using System.Globalization;

namespace UnityEngine;

/// <summary>A 3D vector of integers, such as a tilemap cell.</summary>
public struct Vector3Int : IEquatable<Vector3Int>, IFormattable
{
    private int m_X;
    private int m_Y;
    private int m_Z;

    public Vector3Int(int x, int y)
    {
        m_X = x;
        m_Y = y;
        m_Z = 0;
    }

    public Vector3Int(int x, int y, int z)
    {
        m_X = x;
        m_Y = y;
        m_Z = z;
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

    public int z
    {
        readonly get => m_Z;
        set => m_Z = value;
    }

    public void Set(int x, int y, int z)
    {
        m_X = x;
        m_Y = y;
        m_Z = z;
    }

    public int this[int index]
    {
        readonly get => index switch
        {
            0 => x,
            1 => y,
            2 => z,
            _ => throw new IndexOutOfRangeException($"Invalid Vector3Int index addressed: {index}!"),
        };
        set
        {
            switch (index)
            {
                case 0: x = value; break;
                case 1: y = value; break;
                case 2: z = value; break;
                default: throw new IndexOutOfRangeException($"Invalid Vector3Int index addressed: {index}!");
            }
        }
    }

    public readonly float magnitude => MathF.Sqrt(x * x + y * y + z * z);
    public readonly int sqrMagnitude => x * x + y * y + z * z;

    public static float Distance(Vector3Int a, Vector3Int b) => (a - b).magnitude;
    public static float Distance(ref Vector3Int a, ref Vector3Int b) => Distance(a, b);

    public static Vector3Int Min(Vector3Int lhs, Vector3Int rhs) =>
        new(Mathf.Min(lhs.x, rhs.x), Mathf.Min(lhs.y, rhs.y), Mathf.Min(lhs.z, rhs.z));

    public static Vector3Int Min(ref Vector3Int lhs, ref Vector3Int rhs) => Min(lhs, rhs);

    public static Vector3Int Max(Vector3Int lhs, Vector3Int rhs) =>
        new(Mathf.Max(lhs.x, rhs.x), Mathf.Max(lhs.y, rhs.y), Mathf.Max(lhs.z, rhs.z));

    public static Vector3Int Max(ref Vector3Int lhs, ref Vector3Int rhs) => Max(lhs, rhs);
    public static Vector3Int Scale(Vector3Int a, Vector3Int b) => new(a.x * b.x, a.y * b.y, a.z * b.z);
    public static Vector3Int Scale(ref Vector3Int a, ref Vector3Int b) => Scale(a, b);

    public void Scale(Vector3Int scale)
    {
        x *= scale.x;
        y *= scale.y;
        z *= scale.z;
    }

    public void Scale(ref Vector3Int scale) => Scale(scale);

    /// <summary>Limits each component to the range between <paramref name="min"/> and
    /// <paramref name="max"/>; the minimum is applied first, then the maximum.</summary>
    public void Clamp(Vector3Int min, Vector3Int max)
    {
        x = Math.Min(max.x, Math.Max(min.x, x));
        y = Math.Min(max.y, Math.Max(min.y, y));
        z = Math.Min(max.z, Math.Max(min.z, z));
    }

    public void Clamp(ref Vector3Int min, ref Vector3Int max) => Clamp(min, max);

    public static implicit operator Vector3(Vector3Int v) => new(v.x, v.y, v.z);
    public static explicit operator Vector2Int(Vector3Int v) => new(v.x, v.y);

    public static Vector3Int FloorToInt(Vector3 v) => new(Mathf.FloorToInt(v.x), Mathf.FloorToInt(v.y), Mathf.FloorToInt(v.z));
    public static Vector3Int FloorToInt(ref Vector3 v) => FloorToInt(v);
    public static Vector3Int CeilToInt(Vector3 v) => new(Mathf.CeilToInt(v.x), Mathf.CeilToInt(v.y), Mathf.CeilToInt(v.z));
    public static Vector3Int CeilToInt(ref Vector3 v) => CeilToInt(v);
    public static Vector3Int RoundToInt(Vector3 v) => new(Mathf.RoundToInt(v.x), Mathf.RoundToInt(v.y), Mathf.RoundToInt(v.z));
    public static Vector3Int RoundToInt(ref Vector3 v) => RoundToInt(v);

    public static Vector3Int operator +(Vector3Int a, Vector3Int b) => new(a.x + b.x, a.y + b.y, a.z + b.z);
    public static Vector3Int operator -(Vector3Int a, Vector3Int b) => new(a.x - b.x, a.y - b.y, a.z - b.z);
    public static Vector3Int operator *(Vector3Int a, Vector3Int b) => new(a.x * b.x, a.y * b.y, a.z * b.z);
    public static Vector3Int operator -(Vector3Int a) => new(-a.x, -a.y, -a.z);
    public static Vector3Int operator *(Vector3Int a, int b) => new(a.x * b, a.y * b, a.z * b);
    public static Vector3Int operator *(int a, Vector3Int b) => new(a * b.x, a * b.y, a * b.z);

    /// <summary>Divides each component and rounds the result down.</summary>
    public static Vector3Int operator /(Vector3Int a, int b) =>
        new(Mathf.FloorToInt(a.x / (float)b), Mathf.FloorToInt(a.y / (float)b), Mathf.FloorToInt(a.z / (float)b));

    public static bool operator ==(Vector3Int lhs, Vector3Int rhs) => lhs.x == rhs.x && lhs.y == rhs.y && lhs.z == rhs.z;
    public static bool operator !=(Vector3Int lhs, Vector3Int rhs) => !(lhs == rhs);

    public override readonly bool Equals(object? other) => other is Vector3Int vector && Equals(vector);
    public readonly bool Equals(Vector3Int other) => this == other;
    public readonly bool Equals(ref Vector3Int other) => this == other;

    public override readonly int GetHashCode()
    {
        var yHash = y.GetHashCode();
        var zHash = z.GetHashCode();
        return x.GetHashCode() ^ (yHash << 4) ^ (yHash >> 28) ^ (zHash >> 4) ^ (zHash << 28);
    }

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"({x.ToString(format, formatProvider)}, {y.ToString(format, formatProvider)}, {z.ToString(format, formatProvider)})";
    }

    public static Vector3Int zero => new(0, 0, 0);
    public static Vector3Int one => new(1, 1, 1);
    public static Vector3Int up => new(0, 1, 0);
    public static Vector3Int down => new(0, -1, 0);
    public static Vector3Int left => new(-1, 0, 0);
    public static Vector3Int right => new(1, 0, 0);
    public static Vector3Int forward => new(0, 0, 1);
    public static Vector3Int back => new(0, 0, -1);
}
