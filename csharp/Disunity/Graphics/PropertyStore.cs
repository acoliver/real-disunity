using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>A texture bound to a property, and the part of a render texture the binding reads.</summary>
internal readonly record struct TextureBinding(Texture? Texture, RenderTextureSubElement Element);

/// <summary>A buffer bound as a constant buffer, with the range it exposes.</summary>
internal readonly record struct ConstantBufferBinding(object? Buffer, int Offset, int Size);

/// <summary>
/// The values set on a material, a property block or the shaders' globals, by property ID. Colors share
/// the vector slot, as in Unity, and SetInt writes the float slot. A property that was never set reads
/// as its zero value (matrices read as identity, textures as null).
/// </summary>
internal sealed partial class PropertyStore
{
    /// <summary>Unity limits array properties to this many elements.</summary>
    internal const int MaxArray = 1023;

    internal readonly PropertyMap<float> Floats;
    internal readonly PropertyMap<int> Ints;
    internal readonly PropertyMap<Vector4> Vectors;
    internal readonly PropertyMap<Matrix4x4> Matrices;
    internal readonly PropertyMap<TextureBinding> Textures;
    internal readonly PropertyMap<Vector2> Offsets;
    internal readonly PropertyMap<Vector2> Scales;
    internal readonly Dictionary<int, float[]> FloatArrays = new();
    internal readonly Dictionary<int, Vector4[]> VectorArrays = new();
    internal readonly Dictionary<int, Color[]> ColorArrays = new();
    internal readonly Dictionary<int, Matrix4x4[]> MatrixArrays = new();
    internal readonly Dictionary<int, object?> Buffers = new();
    internal readonly Dictionary<int, ConstantBufferBinding> ConstantBuffers = new();

    /// <summary>Called after a float, integer, vector, matrix or texture property is written, with the property's ID and kind. A
    /// texture's offset and scale report as the texture. Arrays and buffers are not reported.</summary>
    internal Action<int, PropertyKind>? Changed;

    internal PropertyStore()
    {
        Floats = new(id => Changed?.Invoke(id, PropertyKind.Float));
        Ints = new(id => Changed?.Invoke(id, PropertyKind.Int));
        Vectors = new(id => Changed?.Invoke(id, PropertyKind.Vector));
        Matrices = new(id => Changed?.Invoke(id, PropertyKind.Matrix));
        Textures = new(id => Changed?.Invoke(id, PropertyKind.Texture));
        Offsets = new(id => Changed?.Invoke(id, PropertyKind.Texture));
        Scales = new(id => Changed?.Invoke(id, PropertyKind.Texture));
    }

    /// <summary>Called after <see cref="Clear"/> empties the store.</summary>
    internal Action? Cleared;

    internal bool Has(int id) => Floats.ContainsKey(id) || Ints.ContainsKey(id) || Vectors.ContainsKey(id) || Matrices.ContainsKey(id)
        || Textures.ContainsKey(id) || Offsets.ContainsKey(id) || Scales.ContainsKey(id) || FloatArrays.ContainsKey(id)
        || VectorArrays.ContainsKey(id) || ColorArrays.ContainsKey(id) || MatrixArrays.ContainsKey(id)
        || Buffers.ContainsKey(id) || ConstantBuffers.ContainsKey(id);

    internal bool IsEmpty => Floats.Count + Ints.Count + Vectors.Count + Matrices.Count + Textures.Count + Offsets.Count + Scales.Count
        + FloatArrays.Count + VectorArrays.Count + ColorArrays.Count + MatrixArrays.Count + Buffers.Count + ConstantBuffers.Count == 0;

    internal void Clear()
    {
        Floats.Clear(); Ints.Clear(); Vectors.Clear(); Matrices.Clear(); Textures.Clear(); Offsets.Clear(); Scales.Clear();
        FloatArrays.Clear(); VectorArrays.Clear(); ColorArrays.Clear(); MatrixArrays.Clear(); Buffers.Clear(); ConstantBuffers.Clear();
        Cleared?.Invoke();
    }

    internal void CopyFrom(PropertyStore other)
    {
        Clear();
        Merge(other);
    }

    /// <summary>Adds every value of <paramref name="other"/>, replacing a value of the same property.</summary>
    internal void Merge(PropertyStore other)
    {
        foreach (var (id, value) in other.Floats) Floats[id] = value;
        foreach (var (id, value) in other.Ints) Ints[id] = value;
        foreach (var (id, value) in other.Vectors) Vectors[id] = value;
        foreach (var (id, value) in other.Matrices) Matrices[id] = value;
        foreach (var (id, value) in other.Textures) Textures[id] = value;
        foreach (var (id, value) in other.Offsets) Offsets[id] = value;
        foreach (var (id, value) in other.Scales) Scales[id] = value;
        foreach (var (id, value) in other.FloatArrays) FloatArrays[id] = (float[])value.Clone();
        foreach (var (id, value) in other.VectorArrays) VectorArrays[id] = (Vector4[])value.Clone();
        foreach (var (id, value) in other.ColorArrays) ColorArrays[id] = (Color[])value.Clone();
        foreach (var (id, value) in other.MatrixArrays) MatrixArrays[id] = (Matrix4x4[])value.Clone();
        foreach (var (id, value) in other.Buffers) Buffers[id] = value;
        foreach (var (id, value) in other.ConstantBuffers) ConstantBuffers[id] = value;
    }

    /// <summary>A copy of an array to store; Unity rejects null, empty and oversized arrays.</summary>
    internal static T[] Copy<T>(T[]? values, string member)
    {
        ArgumentNullException.ThrowIfNull(values);
        if (values.Length == 0) throw new ArgumentException($"{member}: the array needs at least one element", nameof(values));
        if (values.Length > MaxArray) throw new ArgumentException($"{member}: the array has {values.Length} elements, Unity allows {MaxArray}", nameof(values));
        return (T[])values.Clone();
    }

    internal static T[] Copy<T>(List<T>? values, string member)
    {
        ArgumentNullException.ThrowIfNull(values);
        return Copy(values.ToArray(), member);
    }

    internal static T[]? Clone<T>(T[]? values) => (T[]?)values?.Clone();

    /// <summary>Replaces the list's contents with the array's; a missing array empties the list.</summary>
    internal static void Fill<T>(T[]? source, List<T> destination)
    {
        ArgumentNullException.ThrowIfNull(destination);
        destination.Clear();
        if (source is not null) destination.AddRange(source);
    }
}
