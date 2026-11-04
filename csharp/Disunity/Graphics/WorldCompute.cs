using System.Text;
using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// The calls into the native world that hold compute shaders, compute buffers, the values bound to a kernel and the
/// dispatches a script records. The managed classes keep what they need to answer a call themselves and send every
/// change here; the world holds the contents and the recorded work. A resource belongs to one world, like the other
/// graphics resources (see <see cref="WorldGraphics"/>).
/// </summary>
internal static unsafe class WorldCompute
{
    /// <summary>The kernel number of a value that belongs to the whole compute shader.</summary>
    internal const int ShaderWide = -1;

    private static void Check(DisunityResult result) => DisunityHost.ThrowIfError(result);

    private static byte[] Utf8(string text) => Encoding.UTF8.GetBytes(text);

    /// <summary>Makes the compute shader in the loaded world: its name, the file and object its kernels are in, and its kernels
    /// in index order, each with its group size or null when none is known.</summary>
    internal static WorldGraphics.Link CreateShader(string name, string file, long pathId, IReadOnlyList<(string Name, uint[]? Size)> kernels)
    {
        var world = WorldGraphics.World;
        var text = new StringBuilder(name).Append(file);
        var records = new DisunityComputeKernel[kernels.Count];
        for (var i = 0; i < records.Length; i++)
        {
            var (kernelName, size) = kernels[i];
            records[i] = new DisunityComputeKernel
            {
                name_offset = (uint)Encoding.UTF8.GetByteCount(text.ToString()), name_length = (uint)Encoding.UTF8.GetByteCount(kernelName),
                size_x = size?[0] ?? 0, size_y = size?[1] ?? 0, size_z = size?[2] ?? 0,
            };
            text.Append(kernelName);
        }
        var bytes = Utf8(text.ToString());
        var desc = new DisunityComputeShaderDesc
        {
            path_id = pathId, name_length = (uint)Encoding.UTF8.GetByteCount(name), file_length = (uint)Encoding.UTF8.GetByteCount(file), kernel_count = (uint)records.Length,
        };
        ulong id;
        fixed (byte* pointer = bytes)
        fixed (DisunityComputeKernel* kernelPointer = records)
            Check(NativeBindings.disunity_compute_shader_create(world, &desc, pointer, (uint)bytes.Length, records.Length == 0 ? null : kernelPointer, &id));
        return new WorldGraphics.Link(world, id);
    }

    /// <summary>The thread group size of each kernel as the world holds it now; a kernel whose size is not known reads 0 by 0 by 0.</summary>
    internal static (uint X, uint Y, uint Z)[] KernelSizes(WorldGraphics.Link shader)
    {
        var info = new DisunityComputeShaderInfo();
        Check(NativeBindings.disunity_compute_shader_read(shader.World, shader.Id, &info, null, null));
        var kernels = new DisunityComputeKernel[info.kernel_count];
        var text = new byte[info.text_length];
        fixed (DisunityComputeKernel* kernelPointer = kernels)
        fixed (byte* textPointer = text)
            Check(NativeBindings.disunity_compute_shader_read(shader.World, shader.Id, &info, kernels.Length == 0 ? null : kernelPointer, text.Length == 0 ? null : textPointer));
        return kernels.Select(kernel => (kernel.size_x, kernel.size_y, kernel.size_z)).ToArray();
    }

    internal static WorldGraphics.Link CreateBuffer(int count, int stride, int kind)
    {
        var world = WorldGraphics.World;
        var desc = new DisunityComputeBufferDesc { count = (uint)count, stride = (uint)stride, kind = (uint)kind };
        ulong id;
        Check(NativeBindings.disunity_compute_buffer_create(world, &desc, &id));
        return new WorldGraphics.Link(world, id);
    }

    internal static void DestroyBuffer(WorldGraphics.Link buffer) => Check(NativeBindings.disunity_compute_buffer_destroy(buffer.World, buffer.Id));

    internal static void WriteBuffer(WorldGraphics.Link buffer, long byteOffset, ReadOnlySpan<byte> data)
    {
        fixed (byte* pointer = data)
            Check(NativeBindings.disunity_compute_buffer_write(buffer.World, buffer.Id, (ulong)byteOffset, data.Length == 0 ? null : pointer, (uint)data.Length));
    }

    /// <summary>Runs the host's GPU flush, so the work scripts recorded has executed and the buffers hold its results when this
    /// returns. Without a flush handler nothing happens and a buffer holds what the CPU wrote.</summary>
    internal static void Flush() => Check(NativeBindings.disunity_gpu_flush(WorldGraphics.World));

    internal static void ReadBuffer(WorldGraphics.Link buffer, long byteOffset, Span<byte> data)
    {
        fixed (byte* pointer = data)
            Check(NativeBindings.disunity_compute_buffer_read(buffer.World, buffer.Id, (ulong)byteOffset, data.Length == 0 ? null : pointer, (uint)data.Length));
    }

    internal static void SetFloat(WorldGraphics.Link shader, int kernel, string name, float value)
    {
        var bytes = Utf8(name);
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_compute_float_write(shader.World, shader.Id, kernel, pointer, (uint)bytes.Length, value));
    }

    internal static void SetInt(WorldGraphics.Link shader, int kernel, string name, int value)
    {
        var bytes = Utf8(name);
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_compute_int_write(shader.World, shader.Id, kernel, pointer, (uint)bytes.Length, value));
    }

    internal static void SetVector(WorldGraphics.Link shader, int kernel, string name, Vector4 value)
    {
        var bytes = Utf8(name);
        var values = stackalloc float[] { value.x, value.y, value.z, value.w };
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_compute_vector_write(shader.World, shader.Id, kernel, pointer, (uint)bytes.Length, values));
    }

    internal static void SetMatrix(WorldGraphics.Link shader, int kernel, string name, Matrix4x4 m)
    {
        var bytes = Utf8(name);
        var values = stackalloc float[]
        {
            m.m00, m.m10, m.m20, m.m30, m.m01, m.m11, m.m21, m.m31,
            m.m02, m.m12, m.m22, m.m32, m.m03, m.m13, m.m23, m.m33,
        };
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_compute_matrix_write(shader.World, shader.Id, kernel, pointer, (uint)bytes.Length, values));
    }

    internal static void SetTexture(WorldGraphics.Link shader, int kernel, string name, (uint Kind, ulong Texture) texture)
    {
        var bytes = Utf8(name);
        var slot = new DisunityTextureSlot { kind = texture.Kind, texture = texture.Texture, scale_x = 1, scale_y = 1 };
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_compute_texture_write(shader.World, shader.Id, kernel, pointer, (uint)bytes.Length, &slot));
    }

    /// <summary>Binds the texture the global shader state holds under <paramref name="global"/> when a dispatch runs.</summary>
    internal static void SetTextureFromGlobal(WorldGraphics.Link shader, int kernel, string name, string global)
    {
        var bytes = Utf8(name + global);
        var lengths = new DisunityComputeGlobalTexture { name_length = (uint)Encoding.UTF8.GetByteCount(name), global_length = (uint)Encoding.UTF8.GetByteCount(global) };
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_compute_texture_global_write(shader.World, shader.Id, kernel, &lengths, pointer));
    }

    internal static void BindBuffer(WorldGraphics.Link shader, uint kernel, string name, WorldGraphics.Link buffer)
    {
        var bytes = Utf8(name);
        fixed (byte* pointer = bytes) Check(NativeBindings.disunity_compute_buffer_bind(shader.World, shader.Id, kernel, pointer, (uint)bytes.Length, buffer.Id));
    }

    internal static void Dispatch(WorldGraphics.Link shader, uint kernel, int x, int y, int z) =>
        Check(NativeBindings.disunity_compute_dispatch(shader.World, shader.Id, kernel, (uint)x, (uint)y, (uint)z));
}
