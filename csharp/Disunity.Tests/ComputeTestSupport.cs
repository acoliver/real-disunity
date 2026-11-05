using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;

/// <summary>A host executor for the managed tests: a native flush callback registered on the world, which takes the pending
/// offscreen work through the ABI the way a Rust host does, and writes into the buffers a dispatch bound.</summary>
internal static unsafe class ComputeTestSupport
{
    /// <summary>What the next flush does.</summary>
    internal enum Behavior { Nothing, ExecuteDispatches, Fail, StateKernelSizes }

    internal static Behavior Mode;
    internal static int Flushes;
    internal static readonly List<(ulong Shader, uint Kernel, uint X, uint Y, uint Z)> Executed = [];
    internal static readonly List<string> BindingNames = [];

    internal static void Reset(Behavior mode)
    {
        (Mode, Flushes) = (mode, 0);
        Executed.Clear();
        BindingNames.Clear();
    }

    internal static void Register(ulong world) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_gpu_flush_register(world, &Flush, 0));

    internal static void Unregister(ulong world) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_gpu_flush_register(world, null, 0));

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static DisunityResult Flush(ulong world, ulong user, byte* message, uint capacity, uint* length)
    {
        Flushes++;
        try
        {
            switch (Mode)
            {
                case Behavior.Fail:
                    var text = Encoding.UTF8.GetBytes("the executor has no device");
                    for (var i = 0; i < text.Length && i < capacity; i++) message[i] = text[i];
                    *length = (uint)Math.Min(text.Length, (int)capacity);
                    return new DisunityResult { code = 3 };
                case Behavior.ExecuteDispatches:
                    ExecuteDispatches(world);
                    break;
                case Behavior.StateKernelSizes:
                    StateKernelSizes(world);
                    break;
            }
            return default;
        }
        catch (Exception error)
        {
            var text = Encoding.UTF8.GetBytes(error.Message);
            for (var i = 0; i < text.Length && i < capacity; i++) message[i] = text[i];
            *length = (uint)Math.Min(text.Length, (int)capacity);
            return new DisunityResult { code = 3 };
        }
    }

    private static void ExecuteDispatches(ulong world)
    {
        foreach (var record in TakeWork(world))
        {
            if (record.kind != 3) continue;
            var entries = Bindings(world, record.bindings);
            Executed.Add((record.shader, record.kernel, record.groups_x, record.groups_y, record.groups_z));
            BindingNames.AddRange(entries.Select(entry => entry.Name));
            foreach (var entry in entries.Where(entry => entry.Kind == 5))
            {
                var value = record.kernel + record.groups_x * record.groups_y * record.groups_z;
                DisunityHost.ThrowIfError(NativeBindings.disunity_compute_buffer_write(world, entry.Resource, 0, (byte*)&value, 4));
            }
        }
    }

    private static void StateKernelSizes(ulong world)
    {
        foreach (var record in TakeWork(world))
            if (record.kind == 3)
                DisunityHost.ThrowIfError(NativeBindings.disunity_compute_kernel_size_write(world, record.shader, record.kernel, 8, 4, 2));
    }

    /// <summary>Takes the pending work through the ABI, as a host does.</summary>
    internal static DisunityOffscreenWork[] TakeWork(ulong world)
    {
        uint count;
        DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_take(world, null, 0, &count));
        var records = new DisunityOffscreenWork[count];
        fixed (DisunityOffscreenWork* pointer = records)
            DisunityHost.ThrowIfError(NativeBindings.disunity_offscreen_take(world, count == 0 ? null : pointer, count, &count));
        return records;
    }

    internal readonly record struct Entry(string Name, uint Kind, uint TextureKind, ulong Resource, int Int, float[] Floats);

    /// <summary>The values a dispatch record carries, by its token.</summary>
    internal static Entry[] Bindings(ulong world, uint token)
    {
        DisunityComputeBindingsInfo info = default;
        DisunityHost.ThrowIfError(NativeBindings.disunity_compute_bindings_read(world, token, &info, null, null, null));
        var entries = new DisunityComputeBinding[info.entry_count];
        var floats = new float[info.float_count];
        var text = new byte[info.text_length];
        fixed (DisunityComputeBinding* entryPointer = entries)
        fixed (float* floatPointer = floats)
        fixed (byte* textPointer = text)
            DisunityHost.ThrowIfError(NativeBindings.disunity_compute_bindings_read(world, token, &info,
                entries.Length == 0 ? null : entryPointer, floats.Length == 0 ? null : floatPointer, text.Length == 0 ? null : textPointer));
        string Text(uint offset, uint length) => Encoding.UTF8.GetString(text, (int)offset, (int)length);
        return entries.Select(entry => new Entry(Text(entry.name_offset, entry.name_length), entry.kind, entry.texture_kind, entry.resource, entry.int_value,
            floats.Skip((int)entry.float_offset).Take(entry.kind switch { 0 => 1, 2 => 4, 3 => 16, 4 => 4, _ => 0 }).ToArray())).ToArray();
    }

    /// <summary>A planned compute shader as the loader builds it.</summary>
    internal static ComputeShader Planned(string json)
    {
        using var document = JsonDocument.Parse(json);
        return PlayerComputeShaders.Build(document.RootElement);
    }

    internal const string TerrainPlan = """
        {"type":"ComputeShader","name":"TerrainReadBackShader","file":"resources.assets","path_id":8167,
         "kernels":[{"name":"TerrainReadBackCS","thread_group_size":[8,8,1]},{"name":"Unsized","thread_group_size":null}]}
        """;
}
