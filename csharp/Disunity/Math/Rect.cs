using System.Globalization;

namespace UnityEngine;

/// <summary>
/// A 2D rectangle from <c>(x, y)</c> with a <c>width</c> and <c>height</c>, as Unity defines it.
/// Width and height may be negative; <c>xMin</c> and <c>xMax</c> are then the start and end
/// edges, not the smaller and larger. <c>Contains</c> includes the minimum edges and excludes
/// the maximum edges.
/// </summary>
public struct Rect : IEquatable<Rect>, IFormattable
{
    private float m_XMin;
    private float m_YMin;
    private float m_Width;
    private float m_Height;

    public Rect(float x, float y, float width, float height) => (m_XMin, m_YMin, m_Width, m_Height) = (x, y, width, height);
    public Rect(Vector2 position, Vector2 size) : this(position.x, position.y, size.x, size.y) { }
    public Rect(ref Vector2 position, ref Vector2 size) : this(position.x, position.y, size.x, size.y) { }
    public Rect(Rect source) : this(source.m_XMin, source.m_YMin, source.m_Width, source.m_Height) { }
    public Rect(ref Rect source) : this(source.m_XMin, source.m_YMin, source.m_Width, source.m_Height) { }

    public static Rect zero => new(0, 0, 0, 0);

    public static Rect MinMaxRect(float xmin, float ymin, float xmax, float ymax) => new(xmin, ymin, xmax - xmin, ymax - ymin);

    public void Set(float x, float y, float width, float height) => (m_XMin, m_YMin, m_Width, m_Height) = (x, y, width, height);

    public float x { readonly get => m_XMin; set => m_XMin = value; }
    public float y { readonly get => m_YMin; set => m_YMin = value; }
    public float width { readonly get => m_Width; set => m_Width = value; }
    public float height { readonly get => m_Height; set => m_Height = value; }

    public Vector2 position
    {
        readonly get => new(m_XMin, m_YMin);
        set => (m_XMin, m_YMin) = (value.x, value.y);
    }

    public Vector2 center
    {
        readonly get => new(x + m_Width / 2f, y + m_Height / 2f);
        set => (m_XMin, m_YMin) = (value.x - m_Width / 2f, value.y - m_Height / 2f);
    }

    public Vector2 min
    {
        readonly get => new(xMin, yMin);
        set { xMin = value.x; yMin = value.y; }
    }

    public Vector2 max
    {
        readonly get => new(xMax, yMax);
        set { xMax = value.x; yMax = value.y; }
    }

    public Vector2 size
    {
        readonly get => new(m_Width, m_Height);
        set => (m_Width, m_Height) = (value.x, value.y);
    }

    /// <summary>Setting a minimum edge moves it and keeps the maximum edge where it was.</summary>
    public float xMin
    {
        readonly get => m_XMin;
        set { var oldMax = xMax; m_XMin = value; m_Width = oldMax - m_XMin; }
    }

    public float yMin
    {
        readonly get => m_YMin;
        set { var oldMax = yMax; m_YMin = value; m_Height = oldMax - m_YMin; }
    }

    public float xMax
    {
        readonly get => m_Width + m_XMin;
        set => m_Width = value - m_XMin;
    }

    public float yMax
    {
        readonly get => m_Height + m_YMin;
        set => m_Height = value - m_YMin;
    }

    public readonly float left => m_XMin;
    public readonly float right => m_XMin + m_Width;
    public readonly float top => m_YMin;
    public readonly float bottom => m_YMin + m_Height;

    public readonly bool Contains(Vector2 point) =>
        point.x >= xMin && point.x < xMax && point.y >= yMin && point.y < yMax;

    public readonly bool Contains(ref Vector2 point) => Contains(point);
    public readonly bool Contains(Vector3 point) => Contains(new Vector2(point.x, point.y));
    public readonly bool Contains(ref Vector3 point) => Contains(point);

    /// <summary>With <paramref name="allowInverse"/>, a rectangle of negative size contains the
    /// points between its edges as if its size were positive.</summary>
    public readonly bool Contains(Vector3 point, bool allowInverse)
    {
        if (!allowInverse) return Contains(point);
        var inX = m_Width < 0 ? point.x <= xMin && point.x > xMax : point.x >= xMin && point.x < xMax;
        var inY = m_Height < 0 ? point.y <= yMin && point.y > yMax : point.y >= yMin && point.y < yMax;
        return inX && inY;
    }

    public readonly bool Contains(ref Vector3 point, bool allowInverse) => Contains(point, allowInverse);

    public readonly bool Overlaps(Rect other) =>
        other.xMax > xMin && other.xMin < xMax && other.yMax > yMin && other.yMin < yMax;

    public readonly bool Overlaps(ref Rect other) => Overlaps(other);

    public readonly bool Overlaps(Rect other, bool allowInverse) =>
        allowInverse ? OrderMinMax(this).Overlaps(OrderMinMax(other)) : Overlaps(other);

    public readonly bool Overlaps(ref Rect other, bool allowInverse) => Overlaps(other, allowInverse);

    private static Rect OrderMinMax(Rect rect)
    {
        if (rect.xMin > rect.xMax) (rect.xMin, rect.xMax) = (rect.xMax, rect.xMin);
        if (rect.yMin > rect.yMax) (rect.yMin, rect.yMax) = (rect.yMax, rect.yMin);
        return rect;
    }

    public static Vector2 NormalizedToPoint(Rect rectangle, Vector2 normalizedRectCoordinates) => new(
        Mathf.Lerp(rectangle.x, rectangle.xMax, normalizedRectCoordinates.x),
        Mathf.Lerp(rectangle.y, rectangle.yMax, normalizedRectCoordinates.y));

    public static Vector2 PointToNormalized(Rect rectangle, Vector2 point) => new(
        Mathf.InverseLerp(rectangle.x, rectangle.xMax, point.x),
        Mathf.InverseLerp(rectangle.y, rectangle.yMax, point.y));

    public static Vector2 NormalizedToPoint(ref Rect rectangle, ref Vector2 normalizedRectCoordinates) => NormalizedToPoint(rectangle, normalizedRectCoordinates);
    public static Vector2 PointToNormalized(ref Rect rectangle, ref Vector2 point) => PointToNormalized(rectangle, point);

    public static bool operator ==(Rect lhs, Rect rhs) => lhs.Equals(rhs);
    public static bool operator !=(Rect lhs, Rect rhs) => !lhs.Equals(rhs);

    public readonly bool Equals(Rect other) =>
        x.Equals(other.x) && y.Equals(other.y) && width.Equals(other.width) && height.Equals(other.height);

    public readonly bool Equals(ref Rect other) => Equals(other);

    public override readonly bool Equals(object? other) => other is Rect rect && Equals(rect);

    public override readonly int GetHashCode() =>
        x.GetHashCode() ^ (width.GetHashCode() << 2) ^ (y.GetHashCode() >> 2) ^ (height.GetHashCode() >> 1);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F2" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"(x:{x.ToString(format, formatProvider)}, y:{y.ToString(format, formatProvider)}, width:{width.ToString(format, formatProvider)}, height:{height.ToString(format, formatProvider)})";
    }
}
