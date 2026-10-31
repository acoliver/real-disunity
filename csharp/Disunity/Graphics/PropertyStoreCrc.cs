using System.Runtime.CompilerServices;
using Disunity.Hosting;

namespace UnityEngine;

/// <summary>The bytes <see cref="Material.ComputeCRC"/> hashes for the properties of a store.</summary>
internal sealed partial class PropertyStore
{
    /// <summary>Writes every set property as its kind, name and value, kind by kind and by name within a kind, so the bytes
    /// do not depend on the order the properties were set. A texture or buffer is written as its identity.</summary>
    internal void WriteForCrc(BinaryWriter output)
    {
        WriteKind(output, 1, Floats, (w, v) => w.Write(v));
        WriteKind(output, 2, Ints, (w, v) => w.Write(v));
        WriteKind(output, 3, Vectors, (w, v) => WriteVector(w, v));
        WriteKind(output, 4, Matrices, WriteMatrix);
        WriteKind(output, 5, Textures, (w, v) => { w.Write(v.Texture?.GetInstanceID() ?? 0); w.Write((int)v.Element); });
        WriteKind(output, 6, Offsets, (w, v) => { w.Write(v.x); w.Write(v.y); });
        WriteKind(output, 7, Scales, (w, v) => { w.Write(v.x); w.Write(v.y); });
        WriteKind(output, 8, FloatArrays, (w, v) => WriteArray(w, v, e => w.Write(e)));
        WriteKind(output, 9, VectorArrays, (w, v) => WriteArray(w, v, e => WriteVector(w, e)));
        WriteKind(output, 10, ColorArrays, (w, v) => WriteArray(w, v, e => WriteVector(w, e)));
        WriteKind(output, 11, MatrixArrays, (w, v) => WriteArray(w, v, e => WriteMatrix(w, e)));
        WriteKind(output, 12, Buffers, (w, v) => w.Write(Identity(v)));
        WriteKind(output, 13, ConstantBuffers, (w, v) => { w.Write(Identity(v.Buffer)); w.Write(v.Offset); w.Write(v.Size); });
    }

    private static void WriteKind<T>(BinaryWriter output, byte kind, Dictionary<int, T> values, Action<BinaryWriter, T> write)
    {
        foreach (var (name, value) in values.Select(pair => (Name: Shader.PropertyName(pair.Key) ?? $"#{pair.Key}", pair.Value))
            .OrderBy(entry => entry.Name, StringComparer.Ordinal))
        {
            output.Write(kind);
            output.Write(name);
            write(output, value);
        }
    }

    private static void WriteVector(BinaryWriter output, Vector4 value)
    {
        output.Write(value.x); output.Write(value.y); output.Write(value.z); output.Write(value.w);
    }

    private static void WriteVector(BinaryWriter output, Color value)
    {
        output.Write(value.r); output.Write(value.g); output.Write(value.b); output.Write(value.a);
    }

    private static void WriteMatrix(BinaryWriter output, Matrix4x4 value)
    {
        for (var i = 0; i < 16; i++) output.Write(value[i]);
    }

    private static void WriteArray<T>(BinaryWriter output, T[] values, Action<T> write)
    {
        output.Write(values.Length);
        foreach (var value in values) write(value);
    }

    private static int Identity(object? buffer) => buffer is null ? 0 : buffer is Object engine ? engine.GetInstanceID() : RuntimeHelpers.GetHashCode(buffer);
}
