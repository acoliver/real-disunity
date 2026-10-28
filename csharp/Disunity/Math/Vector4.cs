using System.Globalization;

namespace UnityEngine;

/// <summary>A 4D vector of four floats. <c>==</c> is approximate: two vectors are equal when
/// the squared length of their difference is below <see cref="kEpsilon"/> squared.
/// <c>Equals</c> compares exactly.</summary>
public struct Vector4 : IEquatable<Vector4>, IFormattable
{
    public const float kEpsilon = 0.00001f;

    public float x;
    public float y;
    public float z;
    public float w;

    public Vector4(float x, float y, float z, float w)
    {
        this.x = x;
        this.y = y;
        this.z = z;
        this.w = w;
    }

    public Vector4(float x, float y, float z) : this(x, y, z, 0f) { }
    public Vector4(float x, float y) : this(x, y, 0f, 0f) { }

    public float this[int index]
    {
        readonly get => index switch
        {
            0 => x,
            1 => y,
            2 => z,
            3 => w,
            _ => throw new IndexOutOfRangeException("Invalid Vector4 index!"),
        };
        set
        {
            switch (index)
            {
                case 0: x = value; break;
                case 1: y = value; break;
                case 2: z = value; break;
                case 3: w = value; break;
                default: throw new IndexOutOfRangeException("Invalid Vector4 index!");
            }
        }
    }

    public void Set(float newX, float newY, float newZ, float newW)
    {
        x = newX;
        y = newY;
        z = newZ;
        w = newW;
    }

    public static Vector4 Lerp(Vector4 a, Vector4 b, float t) => LerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Vector4 Lerp(ref Vector4 a, ref Vector4 b, float t) => Lerp(a, b, t);

    public static Vector4 LerpUnclamped(Vector4 a, Vector4 b, float t) =>
        new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t, a.w + (b.w - a.w) * t);

    public static Vector4 LerpUnclamped(ref Vector4 a, ref Vector4 b, float t) => LerpUnclamped(a, b, t);

    public static Vector4 MoveTowards(Vector4 current, Vector4 target, float maxDistanceDelta)
    {
        var toVector = target - current;
        var distance = toVector.magnitude;
        if (distance <= maxDistanceDelta || distance == 0f) return target;
        return current + toVector / distance * maxDistanceDelta;
    }

    public static Vector4 MoveTowards(ref Vector4 current, ref Vector4 target, float maxDistanceDelta) => MoveTowards(current, target, maxDistanceDelta);

    public static Vector4 Scale(Vector4 a, Vector4 b) => new(a.x * b.x, a.y * b.y, a.z * b.z, a.w * b.w);
    public static Vector4 Scale(ref Vector4 a, ref Vector4 b) => Scale(a, b);

    public void Scale(Vector4 scale)
    {
        x *= scale.x;
        y *= scale.y;
        z *= scale.z;
        w *= scale.w;
    }

    public void Scale(ref Vector4 scale) => Scale(scale);

    public override readonly int GetHashCode() =>
        x.GetHashCode() ^ (y.GetHashCode() << 2) ^ (z.GetHashCode() >> 2) ^ (w.GetHashCode() >> 1);

    public override readonly bool Equals(object? other) => other is Vector4 vector && Equals(vector);
    public readonly bool Equals(Vector4 other) => x.Equals(other.x) && y.Equals(other.y) && z.Equals(other.z) && w.Equals(other.w);
    public readonly bool Equals(ref Vector4 other) => Equals(other);

    /// <summary>Vectors shorter than <see cref="kEpsilon"/> become zero.</summary>
    public static Vector4 Normalize(Vector4 a)
    {
        var length = Magnitude(a);
        return length > kEpsilon ? a / length : zero;
    }

    public static Vector4 Normalize(ref Vector4 a) => Normalize(a);
    public void Normalize() => this = Normalize(this);
    public readonly Vector4 normalized => Normalize(this);

    public static float Dot(Vector4 a, Vector4 b) => a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    public static float Dot(ref Vector4 a, ref Vector4 b) => Dot(a, b);

    public static Vector4 Project(Vector4 a, Vector4 b) => b * (Dot(a, b) / Dot(b, b));
    public static Vector4 Project(ref Vector4 a, ref Vector4 b) => Project(a, b);

    public static float Distance(Vector4 a, Vector4 b) => Magnitude(a - b);
    public static float Distance(ref Vector4 a, ref Vector4 b) => Distance(a, b);

    public static float Magnitude(Vector4 a) => MathF.Sqrt(Dot(a, a));
    public static float Magnitude(ref Vector4 a) => Magnitude(a);
    public readonly float magnitude => MathF.Sqrt(Dot(this, this));
    public readonly float sqrMagnitude => Dot(this, this);
    public static float SqrMagnitude(Vector4 a) => Dot(a, a);
    public static float SqrMagnitude(ref Vector4 a) => Dot(a, a);
    public readonly float SqrMagnitude() => Dot(this, this);

    public static Vector4 Min(Vector4 lhs, Vector4 rhs) =>
        new(Mathf.Min(lhs.x, rhs.x), Mathf.Min(lhs.y, rhs.y), Mathf.Min(lhs.z, rhs.z), Mathf.Min(lhs.w, rhs.w));

    public static Vector4 Min(ref Vector4 lhs, ref Vector4 rhs) => Min(lhs, rhs);

    public static Vector4 Max(Vector4 lhs, Vector4 rhs) =>
        new(Mathf.Max(lhs.x, rhs.x), Mathf.Max(lhs.y, rhs.y), Mathf.Max(lhs.z, rhs.z), Mathf.Max(lhs.w, rhs.w));

    public static Vector4 Max(ref Vector4 lhs, ref Vector4 rhs) => Max(lhs, rhs);

    public static Vector4 zero => new(0f, 0f, 0f, 0f);
    public static Vector4 one => new(1f, 1f, 1f, 1f);
    public static Vector4 positiveInfinity => new(float.PositiveInfinity, float.PositiveInfinity, float.PositiveInfinity, float.PositiveInfinity);
    public static Vector4 negativeInfinity => new(float.NegativeInfinity, float.NegativeInfinity, float.NegativeInfinity, float.NegativeInfinity);

    public static Vector4 operator +(Vector4 a, Vector4 b) => new(a.x + b.x, a.y + b.y, a.z + b.z, a.w + b.w);
    public static Vector4 operator -(Vector4 a, Vector4 b) => new(a.x - b.x, a.y - b.y, a.z - b.z, a.w - b.w);
    public static Vector4 operator -(Vector4 a) => new(-a.x, -a.y, -a.z, -a.w);
    public static Vector4 operator *(Vector4 a, float d) => new(a.x * d, a.y * d, a.z * d, a.w * d);
    public static Vector4 operator *(float d, Vector4 a) => new(a.x * d, a.y * d, a.z * d, a.w * d);
    public static Vector4 operator /(Vector4 a, float d) => new(a.x / d, a.y / d, a.z / d, a.w / d);

    public static bool operator ==(Vector4 lhs, Vector4 rhs) => SqrMagnitude(lhs - rhs) < kEpsilon * kEpsilon;
    public static bool operator !=(Vector4 lhs, Vector4 rhs) => !(lhs == rhs);

    public static implicit operator Vector4(Vector3 v) => new(v.x, v.y, v.z, 0f);
    public static implicit operator Vector3(Vector4 v) => new(v.x, v.y, v.z);
    public static implicit operator Vector4(Vector2 v) => new(v.x, v.y, 0f, 0f);
    public static implicit operator Vector2(Vector4 v) => new(v.x, v.y);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F2" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"({x.ToString(format, formatProvider)}, {y.ToString(format, formatProvider)}, {z.ToString(format, formatProvider)}, {w.ToString(format, formatProvider)})";
    }
}
