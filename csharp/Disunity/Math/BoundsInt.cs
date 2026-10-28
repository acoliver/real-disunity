using System.Collections;
using System.Globalization;

namespace UnityEngine;

/// <summary>An integer box from a position with a size. <c>Contains</c> includes the minimum
/// faces and excludes the maximum faces. <c>xMin</c> and <c>xMax</c> are the smaller and larger
/// edge even for a negative size.</summary>
public struct BoundsInt : IEquatable<BoundsInt>, IFormattable
{
    private Vector3Int m_Position;
    private Vector3Int m_Size;

    public BoundsInt(int xMin, int yMin, int zMin, int sizeX, int sizeY, int sizeZ)
    {
        m_Position = new Vector3Int(xMin, yMin, zMin);
        m_Size = new Vector3Int(sizeX, sizeY, sizeZ);
    }

    public BoundsInt(Vector3Int position, Vector3Int size)
    {
        m_Position = position;
        m_Size = size;
    }

    public BoundsInt(ref Vector3Int position, ref Vector3Int size) : this(position, size) { }

    public int x
    {
        readonly get => m_Position.x;
        set => m_Position.x = value;
    }

    public int y
    {
        readonly get => m_Position.y;
        set => m_Position.y = value;
    }

    public int z
    {
        readonly get => m_Position.z;
        set => m_Position.z = value;
    }

    public readonly Vector3 center => new(x + m_Size.x / 2f, y + m_Size.y / 2f, z + m_Size.z / 2f);

    public Vector3Int min
    {
        readonly get => new(xMin, yMin, zMin);
        set
        {
            xMin = value.x;
            yMin = value.y;
            zMin = value.z;
        }
    }

    public Vector3Int max
    {
        readonly get => new(xMax, yMax, zMax);
        set
        {
            xMax = value.x;
            yMax = value.y;
            zMax = value.z;
        }
    }

    /// <summary>Setting a minimum face moves it and keeps the maximum face where it was.</summary>
    public int xMin
    {
        readonly get => Math.Min(m_Position.x, m_Position.x + m_Size.x);
        set
        {
            var oldMax = xMax;
            m_Position.x = value;
            m_Size.x = oldMax - m_Position.x;
        }
    }

    public int yMin
    {
        readonly get => Math.Min(m_Position.y, m_Position.y + m_Size.y);
        set
        {
            var oldMax = yMax;
            m_Position.y = value;
            m_Size.y = oldMax - m_Position.y;
        }
    }

    public int zMin
    {
        readonly get => Math.Min(m_Position.z, m_Position.z + m_Size.z);
        set
        {
            var oldMax = zMax;
            m_Position.z = value;
            m_Size.z = oldMax - m_Position.z;
        }
    }

    public int xMax
    {
        readonly get => Math.Max(m_Position.x, m_Position.x + m_Size.x);
        set => m_Size.x = value - m_Position.x;
    }

    public int yMax
    {
        readonly get => Math.Max(m_Position.y, m_Position.y + m_Size.y);
        set => m_Size.y = value - m_Position.y;
    }

    public int zMax
    {
        readonly get => Math.Max(m_Position.z, m_Position.z + m_Size.z);
        set => m_Size.z = value - m_Position.z;
    }

    public Vector3Int position
    {
        readonly get => m_Position;
        set => m_Position = value;
    }

    public Vector3Int size
    {
        readonly get => m_Size;
        set => m_Size = value;
    }

    public void SetMinMax(Vector3Int minPosition, Vector3Int maxPosition)
    {
        min = minPosition;
        max = maxPosition;
    }

    public void SetMinMax(ref Vector3Int minPosition, ref Vector3Int maxPosition) => SetMinMax(minPosition, maxPosition);

    /// <summary>Moves the box, keeping its size, so that it lies inside <paramref name="bounds"/>
    /// where its size allows.</summary>
    public void ClampToBounds(BoundsInt bounds)
    {
        position = new Vector3Int(
            Math.Max(Math.Min(bounds.xMax - m_Size.x, x), bounds.xMin),
            Math.Max(Math.Min(bounds.yMax - m_Size.y, y), bounds.yMin),
            Math.Max(Math.Min(bounds.zMax - m_Size.z, z), bounds.zMin));
    }

    public void ClampToBounds(ref BoundsInt bounds) => ClampToBounds(bounds);

    public readonly bool Contains(Vector3Int position) =>
        position.x >= xMin && position.y >= yMin && position.z >= zMin
        && position.x < xMax && position.y < yMax && position.z < zMax;

    public readonly bool Contains(ref Vector3Int position) => Contains(position);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"Position: {m_Position.ToString(format, formatProvider)}, Size: {m_Size.ToString(format, formatProvider)}";
    }

    public static bool operator ==(BoundsInt lhs, BoundsInt rhs) => lhs.m_Position == rhs.m_Position && lhs.m_Size == rhs.m_Size;
    public static bool operator !=(BoundsInt lhs, BoundsInt rhs) => !(lhs == rhs);

    public override readonly bool Equals(object? other) => other is BoundsInt bounds && Equals(bounds);
    public readonly bool Equals(BoundsInt other) => this == other;
    public readonly bool Equals(ref BoundsInt other) => this == other;
    public override readonly int GetHashCode() => m_Position.GetHashCode() ^ (m_Size.GetHashCode() << 2);

    /// <summary>Every cell of the box, with x changing fastest, then y, then z.</summary>
    public readonly PositionEnumerator allPositionsWithin => new(min, max);

    public struct PositionEnumerator : IEnumerator<Vector3Int>, IEnumerable<Vector3Int>
    {
        private readonly Vector3Int _min;
        private readonly Vector3Int _max;
        private Vector3Int _current;

        public PositionEnumerator(Vector3Int min, Vector3Int max)
        {
            _min = min;
            _max = max;
            _current = new Vector3Int(min.x - 1, min.y, min.z);
        }

        public readonly PositionEnumerator GetEnumerator() => this;
        readonly IEnumerator<Vector3Int> IEnumerable<Vector3Int>.GetEnumerator() => this;
        readonly IEnumerator IEnumerable.GetEnumerator() => this;

        public readonly Vector3Int Current => _current;
        readonly object IEnumerator.Current => _current;

        public bool MoveNext()
        {
            if (_min.x >= _max.x || _min.y >= _max.y || _min.z >= _max.z) return false;
            _current.x++;
            if (_current.x < _max.x) return true;
            _current.x = _min.x;
            _current.y++;
            if (_current.y < _max.y) return true;
            _current.y = _min.y;
            _current.z++;
            return _current.z < _max.z;
        }

        public void Reset() => _current = new Vector3Int(_min.x - 1, _min.y, _min.z);

        public readonly void Dispose() { }
    }
}
