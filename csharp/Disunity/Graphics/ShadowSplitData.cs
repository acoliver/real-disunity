using System.Runtime.CompilerServices;

namespace UnityEngine.Rendering;

[InlineArray(ShadowSplitData.MaxCullingPlanes)]
internal struct CullingPlanes
{
    private Plane _first;
}

/// <summary>
/// The culling volume of one shadow split: up to 10 planes, a culling sphere and the near plane. Plain data; the
/// engine fills it in only when it culls shadows, which it does not yet.
/// </summary>
public struct ShadowSplitData : IEquatable<ShadowSplitData>
{
    public const int MaxCullingPlanes = 10;

    private CullingPlanes _planes;
    private int _planeCount;

    public Vector4 cullingSphere { get; set; }
    public float shadowCascadeBlendCullingFactor { get; set; }
    public Matrix4x4 cullingMatrix { get; set; }
    public float cullingNearPlane { get; set; }

    /// <summary>How many planes are in use, from 0 to 10; a larger value throws <see cref="ArgumentException"/>.</summary>
    public int cullingPlaneCount
    {
        readonly get => _planeCount;
        set => _planeCount = value is >= 0 and <= MaxCullingPlanes
            ? value : throw new ArgumentException($"culling plane count {value} is outside 0 to {MaxCullingPlanes}", nameof(value));
    }

    public readonly Plane GetCullingPlane(int index) => _planes[CheckPlane(index)];
    public void SetCullingPlane(int index, Plane plane) => _planes[CheckPlane(index)] = plane;

    private readonly int CheckPlane(int index) =>
        (uint)index < (uint)_planeCount ? index : throw new IndexOutOfRangeException($"culling plane {index} is outside the {_planeCount} in use");

    public readonly bool Equals(ShadowSplitData other)
    {
        if (_planeCount != other._planeCount || !cullingSphere.Equals(other.cullingSphere) || !cullingMatrix.Equals(other.cullingMatrix)) return false;
        if (!shadowCascadeBlendCullingFactor.Equals(other.shadowCascadeBlendCullingFactor) || !cullingNearPlane.Equals(other.cullingNearPlane)) return false;
        for (var index = 0; index < _planeCount; index++)
            if (_planes[index].normal != other._planes[index].normal || _planes[index].distance != other._planes[index].distance) return false;
        return true;
    }

    public override readonly bool Equals(object? obj) => obj is ShadowSplitData other && Equals(other);
    public override readonly int GetHashCode() => HashCode.Combine(_planeCount, cullingSphere, shadowCascadeBlendCullingFactor, cullingMatrix, cullingNearPlane);
    public static bool operator ==(ShadowSplitData left, ShadowSplitData right) => left.Equals(right);
    public static bool operator !=(ShadowSplitData left, ShadowSplitData right) => !left.Equals(right);
}
