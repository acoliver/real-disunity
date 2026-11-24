using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using Disunity.Hosting;
using Disunity.Native;
using UnityEngine;

/// <summary>A host executor for the managed tests: a native flush callback registered on the world, which takes the pending
/// offscreen work through the graphics snapshot the way a Rust host does, and writes into the buffers a dispatch bound.</summary>
internal static unsafe class ComputeTestSupport
{
    /// <summary>What the next flush does.</summary>
    internal enum Behavior { Nothing, ExecuteDispatches, Fail, StateKernelSizes }

    internal static Behavior Mode;
    internal static string FailureText = "the executor has no device";
    internal static int Flushes;
    internal static readonly List<(ulong Shader, uint Kernel, uint X, uint Y, uint Z)> Executed = [];
    internal static readonly List<string> BindingNames = [];

    internal static void Reset(Behavior mode)
    {
        (Mode, Flushes) = (mode, 0);
        FailureText = "the executor has no device";
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
                    var text = Encoding.UTF8.GetBytes(FailureText);
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
        foreach (var record in CommandWireReader.Work(world).Where(record => record.Tag == 3))
        {
            Executed.Add((record.Shader, record.Kernel, record.Groups[0], record.Groups[1], record.Groups[2]));
            BindingNames.AddRange(record.Bindings.Select(entry => entry.Name));
            foreach (var entry in record.Bindings.Where(entry => entry.Kind == 5))
            {
                var value = record.Kernel + record.Groups[0] * record.Groups[1] * record.Groups[2];
                DisunityHost.ThrowIfError(NativeBindings.disunity_compute_buffer_write(world, entry.Resource, 0, (byte*)&value, 4));
            }
        }
    }

    private static void StateKernelSizes(ulong world)
    {
        foreach (var record in CommandWireReader.Work(world).Where(record => record.Tag == 3))
            DisunityHost.ThrowIfError(NativeBindings.disunity_compute_kernel_size_write(world, record.Shader, record.Kernel, 8, 4, 2));
    }

    /// <summary>One value a dispatch ran with: <c>Kind</c> 0 float, 1 integer, 2 vector, 3 matrix, 4 texture, 5 buffer, 6 vector
    /// array or 7 matrix array. A texture's <c>TextureKind</c> is 0 none, 1 asset or 2 render texture, and its <c>Floats</c> are
    /// the scale then the offset; a buffer's <c>Resource</c> is the compute buffer's id.</summary>
    internal readonly record struct Entry(string Name, uint Kind, uint TextureKind, ulong Resource, int Int, float[] Floats);

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
