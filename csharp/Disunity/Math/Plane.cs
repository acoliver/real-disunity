using System.Globalization;

namespace UnityEngine;

/// <summary>An infinite plane. A point p is on the plane when <c>Dot(normal, p) + distance</c>
/// is zero, so <see cref="distance"/> is the distance from the origin measured against the
/// normal.</summary>
public struct Plane : IFormattable
{
    private Vector3 m_Normal;
    private float m_Distance;

    public Plane(Vector3 inNormal, Vector3 inPoint)
    {
        m_Normal = Vector3.Normalize(inNormal);
        m_Distance = -Vector3.Dot(m_Normal, inPoint);
    }

    public Plane(ref Vector3 inNormal, ref Vector3 inPoint) : this(inNormal, inPoint) { }

    public Plane(Vector3 inNormal, float d)
    {
        m_Normal = Vector3.Normalize(inNormal);
        m_Distance = d;
    }

    public Plane(ref Vector3 inNormal, float d) : this(inNormal, d) { }

    public Plane(Vector3 a, Vector3 b, Vector3 c)
    {
        m_Normal = Vector3.Normalize(Vector3.Cross(b - a, c - a));
        m_Distance = -Vector3.Dot(m_Normal, a);
    }

    public Plane(ref Vector3 a, ref Vector3 b, ref Vector3 c) : this(a, b, c) { }

    public Vector3 normal
    {
        readonly get => m_Normal;
        set => m_Normal = value;
    }

    public float distance
    {
        readonly get => m_Distance;
        set => m_Distance = value;
    }

    public void SetNormalAndPosition(Vector3 inNormal, Vector3 inPoint)
    {
        m_Normal = Vector3.Normalize(inNormal);
        m_Distance = -Vector3.Dot(m_Normal, inPoint);
    }

    public void SetNormalAndPosition(ref Vector3 inNormal, ref Vector3 inPoint) => SetNormalAndPosition(inNormal, inPoint);

    public void Set3Points(Vector3 a, Vector3 b, Vector3 c)
    {
        m_Normal = Vector3.Normalize(Vector3.Cross(b - a, c - a));
        m_Distance = -Vector3.Dot(m_Normal, a);
    }

    public void Set3Points(ref Vector3 a, ref Vector3 b, ref Vector3 c) => Set3Points(a, b, c);

    public void Flip()
    {
        m_Normal = -m_Normal;
        m_Distance = -m_Distance;
    }

    public readonly Plane flipped => new(-m_Normal, -m_Distance);

    /// <summary>Moves the plane by <paramref name="translation"/>, so a plane through a point
    /// passes through that point plus the translation.</summary>
    public void Translate(Vector3 translation) => m_Distance -= Vector3.Dot(m_Normal, translation);
    public void Translate(ref Vector3 translation) => Translate(translation);

    public static Plane Translate(Plane plane, Vector3 translation) => new(plane.m_Normal, plane.m_Distance - Vector3.Dot(plane.m_Normal, translation));
    public static Plane Translate(ref Plane plane, ref Vector3 translation) => Translate(plane, translation);

    public readonly Vector3 ClosestPointOnPlane(Vector3 point) => point - m_Normal * GetDistanceToPoint(point);
    public readonly Vector3 ClosestPointOnPlane(ref Vector3 point) => ClosestPointOnPlane(point);

    /// <summary>The signed distance: positive on the side the normal points to.</summary>
    public readonly float GetDistanceToPoint(Vector3 point) => Vector3.Dot(m_Normal, point) + m_Distance;
    public readonly float GetDistanceToPoint(ref Vector3 point) => GetDistanceToPoint(point);

    public readonly bool GetSide(Vector3 point) => GetDistanceToPoint(point) > 0f;
    public readonly bool GetSide(ref Vector3 point) => GetSide(point);

    public readonly bool SameSide(Vector3 inPt0, Vector3 inPt1)
    {
        var first = GetDistanceToPoint(inPt0);
        var second = GetDistanceToPoint(inPt1);
        return (first > 0f && second > 0f) || (first <= 0f && second <= 0f);
    }

    public readonly bool SameSide(ref Vector3 inPt0, ref Vector3 inPt1) => SameSide(inPt0, inPt1);

    /// <summary>Sets <paramref name="enter"/> to the distance along the ray to the plane. Returns
    /// false, with a distance of 0 or a negative one, when the ray is parallel to the plane or
    /// points away from it.</summary>
    public readonly bool Raycast(Ray ray, ref float enter)
    {
        var approach = Vector3.Dot(ray.direction, m_Normal);
        var offset = -Vector3.Dot(ray.origin, m_Normal) - m_Distance;
        if (Mathf.Approximately(approach, 0f))
        {
            enter = 0f;
            return false;
        }
        enter = offset / approach;
        return enter > 0f;
    }

    public readonly bool Raycast(ref Ray ray, ref float enter) => Raycast(ray, ref enter);

    public static bool operator ==(Plane lhs, Plane rhs) => lhs.Equals(rhs);
    public static bool operator !=(Plane lhs, Plane rhs) => !lhs.Equals(rhs);

    public override readonly bool Equals(object? other) => other is Plane plane && Equals(plane);
    public readonly bool Equals(Plane other) => m_Normal.Equals(other.m_Normal) && m_Distance.Equals(other.m_Distance);
    public readonly bool Equals(ref Plane other) => Equals(other);
    public override readonly int GetHashCode() => m_Normal.GetHashCode() ^ (m_Distance.GetHashCode() << 2);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F1" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"(normal:{m_Normal.ToString(format, formatProvider)}, distance:{m_Distance.ToString(format, formatProvider)})";
    }
}
