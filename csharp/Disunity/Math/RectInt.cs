using System.Collections;
using System.Globalization;

namespace UnityEngine;

/// <summary>An integer rectangle from <c>(x, y)</c> with a <c>width</c> and <c>height</c>.
/// <c>Contains</c> includes the minimum edges and excludes the maximum edges.
/// <c>xMin</c> and <c>xMax</c> are the smaller and larger edge even for a negative size.</summary>
public struct RectInt : IEquatable<RectInt>, IFormattable
{
    private int m_XMin;
    private int m_YMin;
    private int m_Width;
    private int m_Height;

    public RectInt(int xMin, int yMin, int width, int height)
    {
        m_XMin = xMin;
        m_YMin = yMin;
        m_Width = width;
        m_Height = height;
    }

    public RectInt(Vector2Int position, Vector2Int size) : this(position.x, position.y, size.x, size.y) { }
    public RectInt(ref Vector2Int position, ref Vector2Int size) : this(position.x, position.y, size.x, size.y) { }

    public static RectInt zero => new(0, 0, 0, 0);

    public int x
    {
        readonly get => m_XMin;
        set => m_XMin = value;
    }

    public int y
    {
        readonly get => m_YMin;
        set => m_YMin = value;
    }

    public readonly Vector2 center => new(x + m_Width / 2f, y + m_Height / 2f);

    public Vector2Int min
    {
        readonly get => new(xMin, yMin);
        set
        {
            xMin = value.x;
            yMin = value.y;
        }
    }

    public Vector2Int max
    {
        readonly get => new(xMax, yMax);
        set
        {
            xMax = value.x;
            yMax = value.y;
        }
    }

    public int width
    {
        readonly get => m_Width;
        set => m_Width = value;
    }

    public int height
    {
        readonly get => m_Height;
        set => m_Height = value;
    }

    /// <summary>Setting a minimum edge moves it and keeps the maximum edge where it was.</summary>
    public int xMin
    {
        readonly get => Math.Min(m_XMin, m_XMin + m_Width);
        set
        {
            var oldMax = xMax;
            m_XMin = value;
            m_Width = oldMax - m_XMin;
        }
    }

    public int yMin
    {
        readonly get => Math.Min(m_YMin, m_YMin + m_Height);
        set
        {
            var oldMax = yMax;
            m_YMin = value;
            m_Height = oldMax - m_YMin;
        }
    }

    public int xMax
    {
        readonly get => Math.Max(m_XMin, m_XMin + m_Width);
        set => m_Width = value - m_XMin;
    }

    public int yMax
    {
        readonly get => Math.Max(m_YMin, m_YMin + m_Height);
        set => m_Height = value - m_YMin;
    }

    public Vector2Int position
    {
        readonly get => new(m_XMin, m_YMin);
        set
        {
            m_XMin = value.x;
            m_YMin = value.y;
        }
    }

    public Vector2Int size
    {
        readonly get => new(m_Width, m_Height);
        set
        {
            m_Width = value.x;
            m_Height = value.y;
        }
    }

    public void SetMinMax(Vector2Int minPosition, Vector2Int maxPosition)
    {
        min = minPosition;
        max = maxPosition;
    }

    public void SetMinMax(ref Vector2Int minPosition, ref Vector2Int maxPosition) => SetMinMax(minPosition, maxPosition);

    /// <summary>Moves the rectangle, keeping its size, so that it lies inside
    /// <paramref name="bounds"/> where its size allows.</summary>
    public void ClampToBounds(RectInt bounds)
    {
        position = new Vector2Int(
            Math.Max(Math.Min(bounds.xMax - m_Width, x), bounds.xMin),
            Math.Max(Math.Min(bounds.yMax - m_Height, y), bounds.yMin));
    }

    public void ClampToBounds(ref RectInt bounds) => ClampToBounds(bounds);

    public readonly bool Contains(Vector2Int position) =>
        position.x >= xMin && position.y >= yMin && position.x < xMax && position.y < yMax;

    public readonly bool Contains(ref Vector2Int position) => Contains(position);

    public readonly bool Overlaps(RectInt other) =>
        other.xMin < xMax && other.xMax > xMin && other.yMin < yMax && other.yMax > yMin;

    public readonly bool Overlaps(ref RectInt other) => Overlaps(other);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"(x:{x.ToString(format, formatProvider)}, y:{y.ToString(format, formatProvider)}, width:{width.ToString(format, formatProvider)}, height:{height.ToString(format, formatProvider)})";
    }

    public static bool operator !=(RectInt lhs, RectInt rhs) => !(lhs == rhs);
    public static bool operator ==(RectInt lhs, RectInt rhs) => lhs.x == rhs.x && lhs.y == rhs.y && lhs.width == rhs.width && lhs.height == rhs.height;

    public override readonly int GetHashCode() => m_XMin.GetHashCode() ^ (m_Width.GetHashCode() << 2) ^ (m_YMin.GetHashCode() >> 2) ^ (m_Height.GetHashCode() >> 1);
    public override readonly bool Equals(object? other) => other is RectInt rect && Equals(rect);
    public readonly bool Equals(RectInt other) => this == other;
    public readonly bool Equals(ref RectInt other) => this == other;

    /// <summary>Every cell of the rectangle, with x changing fastest.</summary>
    public readonly PositionEnumerator allPositionsWithin => new(min, max);

    public struct PositionEnumerator : IEnumerator<Vector2Int>, IEnumerable<Vector2Int>
    {
        private readonly Vector2Int _min;
        private readonly Vector2Int _max;
        private Vector2Int _current;

        public PositionEnumerator(Vector2Int min, Vector2Int max)
        {
            _min = min;
            _max = max;
            _current = new Vector2Int(min.x - 1, min.y);
        }

        public readonly PositionEnumerator GetEnumerator() => this;
        readonly IEnumerator<Vector2Int> IEnumerable<Vector2Int>.GetEnumerator() => this;
        readonly IEnumerator IEnumerable.GetEnumerator() => this;

        public readonly Vector2Int Current => _current;
        readonly object IEnumerator.Current => _current;

        public bool MoveNext()
        {
            if (_min.x >= _max.x || _min.y >= _max.y) return false;
            _current.x++;
            if (_current.x < _max.x) return true;
            _current.x = _min.x;
            _current.y++;
            return _current.y < _max.y;
        }

        public void Reset() => _current = new Vector2Int(_min.x - 1, _min.y);

        public readonly void Dispose() { }
    }
}
