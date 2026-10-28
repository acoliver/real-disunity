using System.Globalization;

namespace UnityEngine;

/// <summary>A half line from an origin along a direction. The direction is always stored
/// as a unit vector.</summary>
public struct Ray : IFormattable
{
    private Vector3 m_Origin;
    private Vector3 m_Direction;

    public Ray(Vector3 origin, Vector3 direction)
    {
        m_Origin = origin;
        m_Direction = direction.normalized;
    }

    public Ray(ref Vector3 origin, ref Vector3 direction) : this(origin, direction) { }

    public Vector3 origin
    {
        readonly get => m_Origin;
        set => m_Origin = value;
    }

    public Vector3 direction
    {
        readonly get => m_Direction;
        set => m_Direction = value.normalized;
    }

    public readonly Vector3 GetPoint(float distance) => m_Origin + m_Direction * distance;

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F1" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"Origin: {m_Origin.ToString(format, formatProvider)}, Dir: {m_Direction.ToString(format, formatProvider)}";
    }
}
