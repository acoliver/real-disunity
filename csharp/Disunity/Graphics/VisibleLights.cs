namespace UnityEngine.Rendering;

[Flags]
public enum VisibleLightFlags
{
    None = 0,
    IntersectsNearPlane = 1,
    IntersectsFarPlane = 2,
}

/// <summary>
/// A light that culling found visible. Plain data that only the engine fills in; the engine does not cull yet, so
/// a game gets only default values (a spot light with no color) from its own code.
/// </summary>
public struct VisibleLight : IEquatable<VisibleLight>
{
    public LightType lightType { get; internal set; }
    public Color finalColor { get; internal set; }
    public Rect screenRect { get; internal set; }
    public Matrix4x4 localToWorldMatrix { get; internal set; }
    public float range { get; internal set; }
    public float spotAngle { get; internal set; }
    public Light? light { get; internal set; }
    public VisibleLightFlags flags { get; internal set; }
    public int occlusionMaskChannel { get; internal set; }

    public readonly bool Equals(VisibleLight other) =>
        lightType == other.lightType && finalColor.Equals(other.finalColor) && screenRect.Equals(other.screenRect)
        && localToWorldMatrix.Equals(other.localToWorldMatrix) && range.Equals(other.range) && spotAngle.Equals(other.spotAngle)
        && ReferenceEquals(light, other.light) && flags == other.flags && occlusionMaskChannel == other.occlusionMaskChannel;

    public override readonly bool Equals(object? obj) => obj is VisibleLight other && Equals(other);

    public override readonly int GetHashCode() =>
        HashCode.Combine(lightType, finalColor, screenRect, range, spotAngle, light, flags, occlusionMaskChannel);

    public static bool operator ==(VisibleLight left, VisibleLight right) => left.Equals(right);
    public static bool operator !=(VisibleLight left, VisibleLight right) => !left.Equals(right);
}

/// <summary>A reflection probe that culling found visible. Plain data that only the engine fills in.</summary>
public struct VisibleReflectionProbe : IEquatable<VisibleReflectionProbe>
{
    public Bounds bounds { get; internal set; }
    public Matrix4x4 localToWorldMatrix { get; internal set; }
    public Vector4 hdrData { get; internal set; }
    public Vector3 center { get; internal set; }
    public float blendDistance { get; internal set; }
    public int importance { get; internal set; }
    public bool isBoxProjection { get; internal set; }
    public Texture? texture { get; internal set; }

    public readonly bool Equals(VisibleReflectionProbe other) =>
        bounds.Equals(other.bounds) && localToWorldMatrix.Equals(other.localToWorldMatrix) && hdrData.Equals(other.hdrData)
        && center.Equals(other.center) && blendDistance.Equals(other.blendDistance) && importance == other.importance
        && isBoxProjection == other.isBoxProjection && ReferenceEquals(texture, other.texture);

    public override readonly bool Equals(object? obj) => obj is VisibleReflectionProbe other && Equals(other);

    public override readonly int GetHashCode() =>
        HashCode.Combine(bounds, center, hdrData, blendDistance, importance, isBoxProjection, texture);

    public static bool operator ==(VisibleReflectionProbe left, VisibleReflectionProbe right) => left.Equals(right);
    public static bool operator !=(VisibleReflectionProbe left, VisibleReflectionProbe right) => !left.Equals(right);
}
