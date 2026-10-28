using System.Globalization;

namespace UnityEngine;

/// <summary>An axis-aligned box stored as a center and half-size extents. <c>==</c> compares
/// the center and extents as <see cref="Vector3"/> values, so it is approximate; <c>Equals</c>
/// compares exactly. The box includes its surface for <see cref="Contains(Vector3)"/>.</summary>
public struct Bounds : IEquatable<Bounds>, IFormattable
{
    private Vector3 m_Center;
    private Vector3 m_Extents;

    public Bounds(Vector3 center, Vector3 size)
    {
        m_Center = center;
        m_Extents = size * 0.5f;
    }

    public Bounds(ref Vector3 center, ref Vector3 size) : this(center, size) { }

    public override readonly int GetHashCode() => m_Center.GetHashCode() ^ (m_Extents.GetHashCode() << 2);
    public override readonly bool Equals(object? other) => other is Bounds bounds && Equals(bounds);
    public readonly bool Equals(Bounds other) => m_Center.Equals(other.m_Center) && m_Extents.Equals(other.m_Extents);
    public readonly bool Equals(ref Bounds other) => Equals(other);

    public Vector3 center
    {
        readonly get => m_Center;
        set => m_Center = value;
    }

    public Vector3 size
    {
        readonly get => m_Extents * 2f;
        set => m_Extents = value * 0.5f;
    }

    public Vector3 extents
    {
        readonly get => m_Extents;
        set => m_Extents = value;
    }

    public Vector3 min
    {
        readonly get => m_Center - m_Extents;
        set => SetMinMax(value, max);
    }

    public Vector3 max
    {
        readonly get => m_Center + m_Extents;
        set => SetMinMax(min, value);
    }

    public static bool operator ==(Bounds lhs, Bounds rhs) => lhs.m_Center == rhs.m_Center && lhs.m_Extents == rhs.m_Extents;
    public static bool operator !=(Bounds lhs, Bounds rhs) => !(lhs == rhs);

    public void SetMinMax(Vector3 min, Vector3 max)
    {
        m_Extents = (max - min) * 0.5f;
        m_Center = min + m_Extents;
    }

    public void SetMinMax(ref Vector3 min, ref Vector3 max) => SetMinMax(min, max);

    public void Encapsulate(Vector3 point) => SetMinMax(Vector3.Min(min, point), Vector3.Max(max, point));
    public void Encapsulate(ref Vector3 point) => Encapsulate(point);

    public void Encapsulate(Bounds bounds)
    {
        Encapsulate(bounds.center - bounds.extents);
        Encapsulate(bounds.center + bounds.extents);
    }

    public void Encapsulate(ref Bounds bounds) => Encapsulate(bounds);

    /// <summary>Grows the box by <paramref name="amount"/> in total along each axis.</summary>
    public void Expand(float amount) => m_Extents += new Vector3(amount, amount, amount) * 0.5f;

    public void Expand(Vector3 amount) => m_Extents += amount * 0.5f;
    public void Expand(ref Vector3 amount) => Expand(amount);

    public readonly bool Intersects(Bounds bounds)
    {
        var lo = min;
        var hi = max;
        var otherLo = bounds.min;
        var otherHi = bounds.max;
        return lo.x <= otherHi.x && hi.x >= otherLo.x
            && lo.y <= otherHi.y && hi.y >= otherLo.y
            && lo.z <= otherHi.z && hi.z >= otherLo.z;
    }

    public readonly bool Intersects(ref Bounds bounds) => Intersects(bounds);

    public readonly bool IntersectRay(Ray ray)
    {
        var distance = 0f;
        return IntersectRay(ray, ref distance);
    }

    public readonly bool IntersectRay(ref Ray ray) => IntersectRay(ray);

    /// <summary>Sets <paramref name="distance"/> to where the ray enters the box, or 0 when
    /// its origin is inside. Boxes behind the origin are not hit.</summary>
    public readonly bool IntersectRay(Ray ray, ref float distance)
    {
        var near = 0f;
        var far = float.PositiveInfinity;
        var lo = min;
        var hi = max;
        for (var axis = 0; axis < 3; axis++)
        {
            if (!ClipSlab(ray.origin[axis], ray.direction[axis], lo[axis], hi[axis], ref near, ref far))
            {
                distance = 0f;
                return false;
            }
        }
        distance = near;
        return true;
    }

    public readonly bool IntersectRay(ref Ray ray, ref float distance) => IntersectRay(ray, ref distance);

    private static bool ClipSlab(float origin, float direction, float lo, float hi, ref float near, ref float far)
    {
        if (MathF.Abs(direction) < 1e-20f) return origin >= lo && origin <= hi;
        var t0 = (lo - origin) / direction;
        var t1 = (hi - origin) / direction;
        near = MathF.Max(near, MathF.Min(t0, t1));
        far = MathF.Min(far, MathF.Max(t0, t1));
        return near <= far;
    }

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F1" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"Center: {m_Center.ToString(format, formatProvider)}, Extents: {m_Extents.ToString(format, formatProvider)}";
    }

    /// <summary>True for points inside the box and on its surface.</summary>
    public readonly bool Contains(Vector3 point)
    {
        var lo = min;
        var hi = max;
        return point.x >= lo.x && point.x <= hi.x
            && point.y >= lo.y && point.y <= hi.y
            && point.z >= lo.z && point.z <= hi.z;
    }

    public readonly bool Contains(ref Vector3 point) => Contains(point);

    /// <summary>The squared distance from the point to the box; 0 for points inside.</summary>
    public readonly float SqrDistance(Vector3 point) => (ClosestPoint(point) - point).sqrMagnitude;

    public readonly float SqrDistance(ref Vector3 point) => SqrDistance(point);

    public readonly Vector3 ClosestPoint(Vector3 point) => Vector3.Max(min, Vector3.Min(max, point));
    public readonly Vector3 ClosestPoint(ref Vector3 point) => ClosestPoint(point);
}
