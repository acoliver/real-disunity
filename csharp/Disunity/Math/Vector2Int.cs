using System.Globalization;

namespace UnityEngine;

/// <summary>A 2D vector of integers, such as a grid cell.</summary>
public struct Vector2Int : IEquatable<Vector2Int>, IFormattable
{
    private int m_X;
    private int m_Y;

    public Vector2Int(int x, int y)
    {
        m_X = x;
        m_Y = y;
    }

    public int x
    {
        readonly get => m_X;
        set => m_X = value;
    }

    public int y
    {
        readonly get => m_Y;
        set => m_Y = value;
    }

    public void Set(int x, int y)
    {
        m_X = x;
        m_Y = y;
    }

    public int this[int index]
    {
        readonly get => index switch
        {
            0 => x,
            1 => y,
            _ => throw new IndexOutOfRangeException($"Invalid Vector2Int index addressed: {index}!"),
        };
        set
        {
            switch (index)
            {
                case 0: x = value; break;
                case 1: y = value; break;
                default: throw new IndexOutOfRangeException($"Invalid Vector2Int index addressed: {index}!");
            }
        }
    }

    public readonly float magnitude => MathF.Sqrt(x * x + y * y);
    public readonly int sqrMagnitude => x * x + y * y;

    public static float Distance(Vector2Int a, Vector2Int b) => (a - b).magnitude;
    public static float Distance(ref Vector2Int a, ref Vector2Int b) => Distance(a, b);
    public static Vector2Int Min(Vector2Int lhs, Vector2Int rhs) => new(Mathf.Min(lhs.x, rhs.x), Mathf.Min(lhs.y, rhs.y));
    public static Vector2Int Min(ref Vector2Int lhs, ref Vector2Int rhs) => Min(lhs, rhs);
    public static Vector2Int Max(Vector2Int lhs, Vector2Int rhs) => new(Mathf.Max(lhs.x, rhs.x), Mathf.Max(lhs.y, rhs.y));
    public static Vector2Int Max(ref Vector2Int lhs, ref Vector2Int rhs) => Max(lhs, rhs);
    public static Vector2Int Scale(Vector2Int a, Vector2Int b) => new(a.x * b.x, a.y * b.y);
    public static Vector2Int Scale(ref Vector2Int a, ref Vector2Int b) => Scale(a, b);

    public void Scale(Vector2Int scale)
    {
        x *= scale.x;
        y *= scale.y;
    }

    public void Scale(ref Vector2Int scale) => Scale(scale);

    /// <summary>Limits each component to the range between <paramref name="min"/> and
    /// <paramref name="max"/>; the minimum is applied first, then the maximum.</summary>
    public void Clamp(Vector2Int min, Vector2Int max)
    {
        x = Math.Min(max.x, Math.Max(min.x, x));
        y = Math.Min(max.y, Math.Max(min.y, y));
    }

    public void Clamp(ref Vector2Int min, ref Vector2Int max) => Clamp(min, max);

    public static implicit operator Vector2(Vector2Int v) => new(v.x, v.y);
    public static explicit operator Vector3Int(Vector2Int v) => new(v.x, v.y, 0);

    public static Vector2Int FloorToInt(Vector2 v) => new(Mathf.FloorToInt(v.x), Mathf.FloorToInt(v.y));
    public static Vector2Int FloorToInt(ref Vector2 v) => FloorToInt(v);
    public static Vector2Int CeilToInt(Vector2 v) => new(Mathf.CeilToInt(v.x), Mathf.CeilToInt(v.y));
    public static Vector2Int CeilToInt(ref Vector2 v) => CeilToInt(v);
    public static Vector2Int RoundToInt(Vector2 v) => new(Mathf.RoundToInt(v.x), Mathf.RoundToInt(v.y));
    public static Vector2Int RoundToInt(ref Vector2 v) => RoundToInt(v);

    public static Vector2Int operator -(Vector2Int v) => new(-v.x, -v.y);
    public static Vector2Int operator +(Vector2Int a, Vector2Int b) => new(a.x + b.x, a.y + b.y);
    public static Vector2Int operator -(Vector2Int a, Vector2Int b) => new(a.x - b.x, a.y - b.y);
    public static Vector2Int operator *(Vector2Int a, Vector2Int b) => new(a.x * b.x, a.y * b.y);
    public static Vector2Int operator *(int a, Vector2Int b) => new(a * b.x, a * b.y);
    public static Vector2Int operator *(Vector2Int a, int b) => new(a.x * b, a.y * b);

    /// <summary>Divides each component and rounds the result down.</summary>
    /// <summary>Integer division of each component, which truncates toward zero.</summary>
    public static Vector2Int operator /(Vector2Int a, int b) => new(a.x / b, a.y / b);

    public static bool operator ==(Vector2Int lhs, Vector2Int rhs) => lhs.x == rhs.x && lhs.y == rhs.y;
    public static bool operator !=(Vector2Int lhs, Vector2Int rhs) => !(lhs == rhs);

    public override readonly bool Equals(object? other) => other is Vector2Int vector && Equals(vector);
    public readonly bool Equals(Vector2Int other) => x == other.x && y == other.y;
    public readonly bool Equals(ref Vector2Int other) => Equals(other);
    public override readonly int GetHashCode() => x.GetHashCode() ^ (y.GetHashCode() << 4) ^ (y.GetHashCode() >> 28);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"({x.ToString(format, formatProvider)}, {y.ToString(format, formatProvider)})";
    }

    public static Vector2Int zero => new(0, 0);
    public static Vector2Int one => new(1, 1);
    public static Vector2Int up => new(0, 1);
    public static Vector2Int down => new(0, -1);
    public static Vector2Int left => new(-1, 0);
    public static Vector2Int right => new(1, 0);
}
