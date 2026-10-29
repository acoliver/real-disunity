namespace UnityEngine.Rendering;

/// <summary>The ID a <see cref="BatchRendererGroup"/> gives a registered mesh. The game uses it as an opaque key.</summary>
public struct BatchMeshID : IEquatable<BatchMeshID>, IComparable<BatchMeshID>
{
    public static readonly BatchMeshID Null = default;

    /// <summary>The engine's layout: 0 is no mesh, and a group numbers its meshes from 1.</summary>
    public uint value;

    public readonly int CompareTo(BatchMeshID other) => value.CompareTo(other.value);
    public readonly bool Equals(BatchMeshID other) => value == other.value;
    public override readonly bool Equals(object? obj) => obj is BatchMeshID other && Equals(other);
    public override readonly int GetHashCode() => unchecked((int)value);
    public static bool operator ==(BatchMeshID a, BatchMeshID b) => a.value == b.value;
    public static bool operator !=(BatchMeshID a, BatchMeshID b) => a.value != b.value;
}

/// <summary>The ID a <see cref="BatchRendererGroup"/> gives a registered material. The game uses it as an opaque key.</summary>
public struct BatchMaterialID : IEquatable<BatchMaterialID>, IComparable<BatchMaterialID>
{
    public static readonly BatchMaterialID Null = default;

    /// <summary>The engine's layout: 0 is no material, and a group numbers its materials from 1.</summary>
    public uint value;

    public readonly int CompareTo(BatchMaterialID other) => value.CompareTo(other.value);
    public readonly bool Equals(BatchMaterialID other) => value == other.value;
    public override readonly bool Equals(object? obj) => obj is BatchMaterialID other && Equals(other);
    public override readonly int GetHashCode() => unchecked((int)value);
    public static bool operator ==(BatchMaterialID a, BatchMaterialID b) => a.value == b.value;
    public static bool operator !=(BatchMaterialID a, BatchMaterialID b) => a.value != b.value;
}

/// <summary>The ID a <see cref="BatchRendererGroup"/> gives a batch it holds. The game uses it as an opaque key.</summary>
public struct BatchID : IEquatable<BatchID>, IComparable<BatchID>
{
    public static readonly BatchID Null = default;

    /// <summary>The engine's layout: 0 is no batch, and a group numbers its batches from 1.</summary>
    public uint value;

    public readonly int CompareTo(BatchID other) => value.CompareTo(other.value);
    public readonly bool Equals(BatchID other) => value == other.value;
    public override readonly bool Equals(object? obj) => obj is BatchID other && Equals(other);
    public override readonly int GetHashCode() => unchecked((int)value);
    public static bool operator ==(BatchID a, BatchID b) => a.value == b.value;
    public static bool operator !=(BatchID a, BatchID b) => a.value != b.value;
}
