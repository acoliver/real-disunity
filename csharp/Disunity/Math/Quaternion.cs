using System.Globalization;

namespace UnityEngine;

/// <summary>A rotation stored as four floats. <c>==</c> is true when the dot product of the
/// two quaternions is above 1 - <see cref="kEpsilon"/>, so a quaternion does not equal its
/// negation even though both describe the same rotation. <c>Equals</c> compares exactly.
/// Euler angles are degrees and apply rotation about Z first, then X, then Y.</summary>
public struct Quaternion : IEquatable<Quaternion>, IFormattable
{
    public const float kEpsilon = 0.000001f;

    public float x;
    public float y;
    public float z;
    public float w;

    public Quaternion(float x, float y, float z, float w)
    {
        this.x = x;
        this.y = y;
        this.z = z;
        this.w = w;
    }

    public float this[int index]
    {
        readonly get => index switch
        {
            0 => x,
            1 => y,
            2 => z,
            3 => w,
            _ => throw new IndexOutOfRangeException("Invalid Quaternion index!"),
        };
        set
        {
            switch (index)
            {
                case 0: x = value; break;
                case 1: y = value; break;
                case 2: z = value; break;
                case 3: w = value; break;
                default: throw new IndexOutOfRangeException("Invalid Quaternion index!");
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

    public static Quaternion identity => new(0f, 0f, 0f, 1f);

    public static Quaternion operator *(Quaternion lhs, Quaternion rhs) => new(
        lhs.w * rhs.x + lhs.x * rhs.w + lhs.y * rhs.z - lhs.z * rhs.y,
        lhs.w * rhs.y + lhs.y * rhs.w + lhs.z * rhs.x - lhs.x * rhs.z,
        lhs.w * rhs.z + lhs.z * rhs.w + lhs.x * rhs.y - lhs.y * rhs.x,
        lhs.w * rhs.w - lhs.x * rhs.x - lhs.y * rhs.y - lhs.z * rhs.z);

    public static Vector3 operator *(Quaternion rotation, Vector3 point)
    {
        var axis = new Vector3(rotation.x, rotation.y, rotation.z);
        var twiceCross = 2f * Vector3.Cross(axis, point);
        return point + rotation.w * twiceCross + Vector3.Cross(axis, twiceCross);
    }

    public static bool operator ==(Quaternion lhs, Quaternion rhs) => Dot(lhs, rhs) > 1f - kEpsilon;
    public static bool operator !=(Quaternion lhs, Quaternion rhs) => !(lhs == rhs);

    public static float Dot(Quaternion a, Quaternion b) => a.x * b.x + a.y * b.y + a.z * b.z + a.w * b.w;
    public static float Dot(ref Quaternion a, ref Quaternion b) => Dot(a, b);

    /// <summary>The angle between the rotations in degrees. Opposite signs of the same
    /// rotation give 0.</summary>
    public static float Angle(Quaternion a, Quaternion b)
    {
        var dot = Mathf.Min(Mathf.Abs(Dot(a, b)), 1f);
        return dot > 1f - kEpsilon ? 0f : MathF.Acos(dot) * 2f * Mathf.Rad2Deg;
    }

    public static float Angle(ref Quaternion a, ref Quaternion b) => Angle(a, b);

    public static Quaternion AngleAxis(float angle, Vector3 axis)
    {
        var direction = Vector3.Normalize(axis);
        if (direction == Vector3.zero) return identity;
        var half = angle * Mathf.Deg2Rad * 0.5f;
        var s = MathF.Sin(half);
        return new Quaternion(direction.x * s, direction.y * s, direction.z * s, MathF.Cos(half));
    }

    public static Quaternion AngleAxis(float angle, ref Vector3 axis) => AngleAxis(angle, axis);

    /// <summary>Gives the angle in degrees and the axis of the rotation; the identity gives
    /// angle 0 about the X axis.</summary>
    public readonly void ToAngleAxis(ref float angle, ref Vector3 axis)
    {
        var q = Normalize(this);
        var clamped = Mathf.Clamp(q.w, -1f, 1f);
        var s = MathF.Sqrt(1f - clamped * clamped);
        angle = 2f * MathF.Acos(clamped) * Mathf.Rad2Deg;
        axis = s < kEpsilon ? new Vector3(1f, 0f, 0f) : new Vector3(q.x / s, q.y / s, q.z / s);
    }

    public static Quaternion FromToRotation(Vector3 fromDirection, Vector3 toDirection)
    {
        var from = Vector3.Normalize(fromDirection);
        var to = Vector3.Normalize(toDirection);
        if (from == Vector3.zero || to == Vector3.zero) return identity;
        var dot = Vector3.Dot(from, to);
        if (dot > 0.999999f) return identity;
        if (dot < -0.999999f)
        {
            var axis = Vector3.Normalize(Vector3.Cross(from, MathF.Abs(from.x) < 0.9f ? Vector3.right : Vector3.up));
            return new Quaternion(axis.x, axis.y, axis.z, 0f);
        }
        var cross = Vector3.Cross(from, to);
        return Normalize(new Quaternion(cross.x, cross.y, cross.z, 1f + dot));
    }

    public static Quaternion FromToRotation(ref Vector3 fromDirection, ref Vector3 toDirection) => FromToRotation(fromDirection, toDirection);
    public void SetFromToRotation(Vector3 fromDirection, Vector3 toDirection) => this = FromToRotation(fromDirection, toDirection);
    public void SetFromToRotation(ref Vector3 fromDirection, ref Vector3 toDirection) => this = FromToRotation(fromDirection, toDirection);

    /// <summary>A rotation whose Z axis points along <paramref name="forward"/> and whose Y
    /// axis is as close to <paramref name="upwards"/> as possible. A zero forward gives the
    /// identity.</summary>
    public static Quaternion LookRotation(Vector3 forward, Vector3 upwards)
    {
        var zAxis = Vector3.Normalize(forward);
        if (zAxis == Vector3.zero) return identity;
        var xAxis = Vector3.Normalize(Vector3.Cross(upwards, zAxis));
        if (xAxis == Vector3.zero)
            xAxis = Vector3.Normalize(Vector3.Cross(MathF.Abs(zAxis.y) < 0.9f ? Vector3.up : Vector3.forward, zAxis));
        return FromBasis(xAxis, Vector3.Cross(zAxis, xAxis), zAxis);
    }

    public static Quaternion LookRotation(ref Vector3 forward, ref Vector3 upwards) => LookRotation(forward, upwards);
    public static Quaternion LookRotation(Vector3 forward) => LookRotation(forward, Vector3.up);
    public static Quaternion LookRotation(ref Vector3 forward) => LookRotation(forward, Vector3.up);
    public void SetLookRotation(Vector3 view) => this = LookRotation(view, Vector3.up);
    public void SetLookRotation(ref Vector3 view) => this = LookRotation(view, Vector3.up);
    public void SetLookRotation(Vector3 view, Vector3 up) => this = LookRotation(view, up);
    public void SetLookRotation(ref Vector3 view, ref Vector3 up) => this = LookRotation(view, up);

    private static Quaternion FromBasis(Vector3 xAxis, Vector3 yAxis, Vector3 zAxis)
    {
        float m00 = xAxis.x, m10 = xAxis.y, m20 = xAxis.z;
        float m01 = yAxis.x, m11 = yAxis.y, m21 = yAxis.z;
        float m02 = zAxis.x, m12 = zAxis.y, m22 = zAxis.z;
        var trace = m00 + m11 + m22;
        if (trace > 0f)
        {
            var s = MathF.Sqrt(trace + 1f) * 2f;
            return new Quaternion((m21 - m12) / s, (m02 - m20) / s, (m10 - m01) / s, 0.25f * s);
        }
        if (m00 >= m11 && m00 >= m22)
        {
            var s = MathF.Sqrt(1f + m00 - m11 - m22) * 2f;
            return new Quaternion(0.25f * s, (m01 + m10) / s, (m02 + m20) / s, (m21 - m12) / s);
        }
        if (m11 >= m22)
        {
            var s = MathF.Sqrt(1f + m11 - m00 - m22) * 2f;
            return new Quaternion((m01 + m10) / s, 0.25f * s, (m12 + m21) / s, (m02 - m20) / s);
        }
        var t = MathF.Sqrt(1f + m22 - m00 - m11) * 2f;
        return new Quaternion((m02 + m20) / t, (m12 + m21) / t, 0.25f * t, (m10 - m01) / t);
    }

    public static Quaternion Inverse(Quaternion rotation)
    {
        var squared = Dot(rotation, rotation);
        if (squared < kEpsilon) return new Quaternion(-rotation.x, -rotation.y, -rotation.z, rotation.w);
        return new Quaternion(-rotation.x / squared, -rotation.y / squared, -rotation.z / squared, rotation.w / squared);
    }

    public static Quaternion Inverse(ref Quaternion rotation) => Inverse(rotation);

    public static Quaternion Slerp(Quaternion a, Quaternion b, float t) => SlerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Quaternion Slerp(ref Quaternion a, ref Quaternion b, float t) => Slerp(a, b, t);

    /// <summary>Interpolates along the shortest arc. Nearly parallel rotations fall back to
    /// a normalized linear blend.</summary>
    public static Quaternion SlerpUnclamped(Quaternion a, Quaternion b, float t)
    {
        var dot = Dot(a, b);
        if (dot < 0f)
        {
            b = new Quaternion(-b.x, -b.y, -b.z, -b.w);
            dot = -dot;
        }
        if (dot > 0.9995f) return LerpUnclamped(a, b, t);
        var angle = MathF.Acos(dot);
        var scaleA = MathF.Sin((1f - t) * angle) / MathF.Sin(angle);
        var scaleB = MathF.Sin(t * angle) / MathF.Sin(angle);
        return Blend(a, scaleA, b, scaleB);
    }

    public static Quaternion SlerpUnclamped(ref Quaternion a, ref Quaternion b, float t) => SlerpUnclamped(a, b, t);

    public static Quaternion Lerp(Quaternion a, Quaternion b, float t) => LerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Quaternion Lerp(ref Quaternion a, ref Quaternion b, float t) => Lerp(a, b, t);

    /// <summary>A normalized linear blend along the shortest arc.</summary>
    public static Quaternion LerpUnclamped(Quaternion a, Quaternion b, float t)
    {
        var sign = Dot(a, b) < 0f ? -1f : 1f;
        return Normalize(Blend(a, 1f - t, b, t * sign));
    }

    public static Quaternion LerpUnclamped(ref Quaternion a, ref Quaternion b, float t) => LerpUnclamped(a, b, t);

    private static Quaternion Blend(Quaternion a, float scaleA, Quaternion b, float scaleB) => new(
        a.x * scaleA + b.x * scaleB, a.y * scaleA + b.y * scaleB, a.z * scaleA + b.z * scaleB, a.w * scaleA + b.w * scaleB);

    public static Quaternion RotateTowards(Quaternion from, Quaternion to, float maxDegreesDelta)
    {
        var angle = Angle(from, to);
        if (angle == 0f) return to;
        return SlerpUnclamped(from, to, Mathf.Min(1f, maxDegreesDelta / angle));
    }

    public static Quaternion RotateTowards(ref Quaternion from, ref Quaternion to, float maxDegreesDelta) => RotateTowards(from, to, maxDegreesDelta);

    /// <summary>Scales to unit length; a quaternion of zero length becomes the identity.</summary>
    public static Quaternion Normalize(Quaternion q)
    {
        var length = MathF.Sqrt(Dot(q, q));
        if (length < Mathf.Epsilon) return identity;
        return new Quaternion(q.x / length, q.y / length, q.z / length, q.w / length);
    }

    public static Quaternion Normalize(ref Quaternion q) => Normalize(q);
    public void Normalize() => this = Normalize(this);
    public readonly Quaternion normalized => Normalize(this);

    /// <summary>The rotation for Euler angles in degrees: about Z, then X, then Y.</summary>
    public static Quaternion Euler(float x, float y, float z)
    {
        float hx = x * Mathf.Deg2Rad * 0.5f, hy = y * Mathf.Deg2Rad * 0.5f, hz = z * Mathf.Deg2Rad * 0.5f;
        float sx = MathF.Sin(hx), cx = MathF.Cos(hx);
        float sy = MathF.Sin(hy), cy = MathF.Cos(hy);
        float sz = MathF.Sin(hz), cz = MathF.Cos(hz);
        return new Quaternion(
            cz * cy * sx + cx * sy * sz,
            cz * cx * sy - cy * sx * sz,
            cy * cx * sz - cz * sy * sx,
            cy * cx * cz + sy * sx * sz);
    }

    public static Quaternion Euler(Vector3 euler) => Euler(euler.x, euler.y, euler.z);
    public static Quaternion Euler(ref Vector3 euler) => Euler(euler.x, euler.y, euler.z);

    /// <summary>The Euler angles in degrees, each in [0, 360). The Y rotation is taken when X is
    /// at plus or minus 90 degrees and Z is reported as 0.</summary>
    public Vector3 eulerAngles
    {
        readonly get => EulerDegrees(this);
        set => this = Euler(value);
    }

    private static Vector3 EulerDegrees(Quaternion rotation)
    {
        var q = Normalize(rotation);
        var m12 = 2f * (q.y * q.z - q.w * q.x);
        float pitch, yaw, roll;
        if (MathF.Abs(m12) > 0.999999f)
        {
            pitch = m12 > 0f ? -MathF.PI / 2f : MathF.PI / 2f;
            yaw = MathF.Atan2(-2f * (q.x * q.z - q.w * q.y), 1f - 2f * (q.y * q.y + q.z * q.z));
            roll = 0f;
        }
        else
        {
            pitch = MathF.Asin(-m12);
            yaw = MathF.Atan2(2f * (q.x * q.z + q.w * q.y), 1f - 2f * (q.x * q.x + q.y * q.y));
            roll = MathF.Atan2(2f * (q.x * q.y + q.w * q.z), 1f - 2f * (q.x * q.x + q.z * q.z));
        }
        return new Vector3(WrapDegrees(pitch), WrapDegrees(yaw), WrapDegrees(roll));
    }

    private static float WrapDegrees(float radians)
    {
        var degrees = radians * Mathf.Rad2Deg;
        if (degrees < 0f) degrees += 360f;
        return degrees >= 360f ? 0f : degrees;
    }

    public override readonly int GetHashCode() => x.GetHashCode() ^ (y.GetHashCode() << 2) ^ (z.GetHashCode() >> 2) ^ (w.GetHashCode() >> 1);
    public override readonly bool Equals(object? other) => other is Quaternion q && Equals(q);
    public readonly bool Equals(Quaternion other) => x.Equals(other.x) && y.Equals(other.y) && z.Equals(other.z) && w.Equals(other.w);
    public readonly bool Equals(ref Quaternion other) => Equals(other);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F5" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"({x.ToString(format, formatProvider)}, {y.ToString(format, formatProvider)}, {z.ToString(format, formatProvider)}, {w.ToString(format, formatProvider)})";
    }

    // Unity's deprecated spellings. The ones that took or returned angles used radians.

    /// <summary>Euler rotation with angles in radians.</summary>
    public static Quaternion EulerRotation(float x, float y, float z) => Euler(x * Mathf.Rad2Deg, y * Mathf.Rad2Deg, z * Mathf.Rad2Deg);
    public static Quaternion EulerRotation(Vector3 euler) => EulerRotation(euler.x, euler.y, euler.z);
    public void SetEulerRotation(float x, float y, float z) => this = EulerRotation(x, y, z);
    public void SetEulerRotation(Vector3 euler) => this = EulerRotation(euler.x, euler.y, euler.z);

    /// <summary>The Euler angles in radians.</summary>
    public readonly Vector3 ToEuler() => EulerDegrees(this) * Mathf.Deg2Rad;
    public static Quaternion EulerAngles(float x, float y, float z) => EulerRotation(x, y, z);
    public static Quaternion EulerAngles(Vector3 euler) => EulerRotation(euler.x, euler.y, euler.z);
    public void SetEulerAngles(float x, float y, float z) => this = EulerRotation(x, y, z);
    public void SetEulerAngles(Vector3 euler) => this = EulerRotation(euler.x, euler.y, euler.z);
    public static Vector3 ToEulerAngles(Quaternion rotation) => rotation.ToEuler();
    public readonly Vector3 ToEulerAngles() => ToEuler();

    /// <summary>Gives the axis and the angle in radians.</summary>
    public readonly void ToAxisAngle(ref Vector3 axis, ref float angle)
    {
        var degrees = 0f;
        ToAngleAxis(ref degrees, ref axis);
        angle = degrees * Mathf.Deg2Rad;
    }

    /// <summary>Sets a rotation about <paramref name="axis"/> by an angle in radians.</summary>
    public void SetAxisAngle(Vector3 axis, float angle) => this = AxisAngle(axis, angle);
    public static Quaternion AxisAngle(Vector3 axis, float angle) => AngleAxis(angle * Mathf.Rad2Deg, axis);
}
