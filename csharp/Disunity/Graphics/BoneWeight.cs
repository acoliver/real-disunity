namespace UnityEngine;

/// <summary>The bones that move one vertex and how much each does: up to four bone indices and their weights. The
/// weights of a vertex that is skinned sum to 1; the engine does not check it.</summary>
public struct BoneWeight : IEquatable<BoneWeight>
{
    public float weight0;
    public float weight1;
    public float weight2;
    public float weight3;
    public int boneIndex0;
    public int boneIndex1;
    public int boneIndex2;
    public int boneIndex3;

    public readonly bool Equals(BoneWeight other) =>
        weight0 == other.weight0 && weight1 == other.weight1 && weight2 == other.weight2 && weight3 == other.weight3
        && boneIndex0 == other.boneIndex0 && boneIndex1 == other.boneIndex1 && boneIndex2 == other.boneIndex2 && boneIndex3 == other.boneIndex3;

    public override readonly bool Equals(object? obj) => obj is BoneWeight other && Equals(other);

    public override readonly int GetHashCode() =>
        HashCode.Combine(HashCode.Combine(weight0, weight1, weight2, weight3), HashCode.Combine(boneIndex0, boneIndex1, boneIndex2, boneIndex3));

    public static bool operator ==(BoneWeight lhs, BoneWeight rhs) => lhs.Equals(rhs);

    public static bool operator !=(BoneWeight lhs, BoneWeight rhs) => !lhs.Equals(rhs);
}
