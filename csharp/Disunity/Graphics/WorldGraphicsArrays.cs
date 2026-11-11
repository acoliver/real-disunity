using System.Text;
using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// The calls that send ordered vector and matrix global arrays to the world: an immediate write of the world's global shader
/// state and a record of ordered offscreen work. Both copy the floats before they return. A matrix goes column by column
/// (element <c>column * 4 + row</c>), as the world holds it. The values were checked to be finite when the caller handed them
/// over, so nothing here substitutes a number for a NaN or an infinity.
/// </summary>
internal static unsafe partial class WorldGraphics
{
    /// <summary>The most bytes a property name holds in the world.</summary>
    private const int MaxArrayNameBytes = 256;

    /// <summary>Checks a name before an array is stored or recorded, so the world never refuses it later, when a buffer is
    /// executed or a world is loaded.</summary>
    internal static void CheckArrayName(string name, string member)
    {
        ArgumentNullException.ThrowIfNull(name);
        var bytes = Encoding.UTF8.GetByteCount(name);
        if (bytes is 0 or > MaxArrayNameBytes)
            throw new ArgumentException($"{member}: a property name has 1 to {MaxArrayNameBytes} UTF-8 bytes, not {bytes}", nameof(name));
    }

    private static float[] Flatten(Vector4[] values)
    {
        var floats = new float[values.Length * 4];
        for (var index = 0; index < values.Length; index++)
            (floats[index * 4], floats[index * 4 + 1], floats[index * 4 + 2], floats[index * 4 + 3]) = (values[index].x, values[index].y, values[index].z, values[index].w);
        return floats;
    }

    private static float[] Flatten(Matrix4x4[] values)
    {
        var floats = new float[values.Length * 16];
        for (var index = 0; index < values.Length; index++)
            for (var column = 0; column < 4; column++)
                for (var row = 0; row < 4; row++)
                    floats[index * 16 + column * 4 + row] = values[index][row, column];
        return floats;
    }

    /// <summary>Sets a global vector array in the loaded world's global shader state.</summary>
    internal static void WriteGlobalVectorArray(string name, Vector4[] values)
    {
        var (bytes, floats) = (Encoding.UTF8.GetBytes(name), Flatten(values));
        fixed (byte* text = bytes)
        fixed (float* data = floats)
            Check(NativeBindings.disunity_shader_global_vector_array_write(DisunityHost.WorldHandle, text, (uint)bytes.Length, data, (uint)values.Length));
    }

    /// <summary>Sets a global matrix array in the loaded world's global shader state.</summary>
    internal static void WriteGlobalMatrixArray(string name, Matrix4x4[] values)
    {
        var (bytes, floats) = (Encoding.UTF8.GetBytes(name), Flatten(values));
        fixed (byte* text = bytes)
        fixed (float* data = floats)
            Check(NativeBindings.disunity_shader_global_matrix_array_write(DisunityHost.WorldHandle, text, (uint)bytes.Length, data, (uint)values.Length));
    }

    /// <summary>Records a global vector array as ordered offscreen work.</summary>
    internal static void RecordGlobalVectorArray(string name, Vector4[] values)
    {
        var (bytes, floats) = (Encoding.UTF8.GetBytes(name), Flatten(values));
        fixed (byte* text = bytes)
        fixed (float* data = floats)
            Check(NativeBindings.disunity_offscreen_set_global_vector_array(World, text, (uint)bytes.Length, data, (uint)values.Length));
    }

    /// <summary>Records a global matrix array as ordered offscreen work.</summary>
    internal static void RecordGlobalMatrixArray(string name, Matrix4x4[] values)
    {
        var (bytes, floats) = (Encoding.UTF8.GetBytes(name), Flatten(values));
        fixed (byte* text = bytes)
        fixed (float* data = floats)
            Check(NativeBindings.disunity_offscreen_set_global_matrix_array(World, text, (uint)bytes.Length, data, (uint)values.Length));
    }

    /// <summary>Sends every vector and matrix array of the global store to the world just loaded. The arrays of a material or a
    /// property block are not sent: only the global state takes them.</summary>
    internal static void SendGlobalArrays(PropertyStore store)
    {
        foreach (var (id, values) in store.VectorArrays)
            if (Shader.PropertyName(id) is { } name) WriteGlobalVectorArray(name, values);
        foreach (var (id, values) in store.MatrixArrays)
            if (Shader.PropertyName(id) is { } name) WriteGlobalMatrixArray(name, values);
    }
}
