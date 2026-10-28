using System.Globalization;

namespace UnityEngine;

/// <summary>A 3D vector of three floats, laid out as the engine's native code expects.
/// <c>==</c> is approximate: two vectors are equal when the squared length of their
/// difference is below <see cref="kEpsilon"/> squared. <c>Equals</c> compares exactly.</summary>
public struct Vector3 : IEquatable<Vector3>, IFormattable
{
    public const float kEpsilon = 0.00001f;
    public const float kEpsilonNormalSqrt = 1e-15f;

    public float x;
    public float y;
    public float z;

    public Vector3(float x, float y, float z)
    {
        this.x = x;
        this.y = y;
        this.z = z;
    }

    public Vector3(float x, float y)
    {
        this.x = x;
        this.y = y;
        z = 0f;
    }

    public float this[int index]
    {
        readonly get => index switch
        {
            0 => x,
            1 => y,
            2 => z,
            _ => throw new IndexOutOfRangeException("Invalid Vector3 index!"),
        };
        set
        {
            switch (index)
            {
                case 0: x = value; break;
                case 1: y = value; break;
                case 2: z = value; break;
                default: throw new IndexOutOfRangeException("Invalid Vector3 index!");
            }
        }
    }

    public void Set(float newX, float newY, float newZ)
    {
        x = newX;
        y = newY;
        z = newZ;
    }

    public static Vector3 Lerp(Vector3 a, Vector3 b, float t) => LerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Vector3 Lerp(ref Vector3 a, ref Vector3 b, float t) => Lerp(a, b, t);

    public static Vector3 LerpUnclamped(Vector3 a, Vector3 b, float t) =>
        new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t, a.z + (b.z - a.z) * t);

    public static Vector3 LerpUnclamped(ref Vector3 a, ref Vector3 b, float t) => LerpUnclamped(a, b, t);

    public static Vector3 Slerp(Vector3 a, Vector3 b, float t) => SlerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Vector3 Slerp(ref Vector3 a, ref Vector3 b, float t) => Slerp(a, b, t);

    /// <summary>Interpolates the direction along the arc between the vectors and the length
    /// linearly. Opposite vectors rotate about an arbitrary perpendicular axis.</summary>
    public static Vector3 SlerpUnclamped(Vector3 a, Vector3 b, float t)
    {
        var lengthA = a.magnitude;
        var lengthB = b.magnitude;
        if (lengthA < kEpsilon || lengthB < kEpsilon) return LerpUnclamped(a, b, t);
        return RotateDirection(a / lengthA, b / lengthB, t) * Mathf.LerpUnclamped(lengthA, lengthB, t);
    }

    public static Vector3 SlerpUnclamped(ref Vector3 a, ref Vector3 b, float t) => SlerpUnclamped(a, b, t);

    private static Vector3 RotateDirection(Vector3 from, Vector3 to, float t)
    {
        var dot = Mathf.Clamp(Dot(from, to), -1f, 1f);
        if (dot > 0.99999f) return Normalize(LerpUnclamped(from, to, t));
        if (dot < -0.99999f)
        {
            var axis = Normalize(Cross(from, AnyPerpendicular(from)));
            return Quaternion.AngleAxis(180f * t, axis) * from;
        }
        var angle = MathF.Acos(dot);
        return (from * MathF.Sin((1f - t) * angle) + to * MathF.Sin(t * angle)) / MathF.Sin(angle);
    }

    private static Vector3 AnyPerpendicular(Vector3 v) =>
        MathF.Abs(v.x) < 0.9f ? Cross(v, right) : Cross(v, up);

    /// <summary>Makes <paramref name="normal"/> unit length and <paramref name="tangent"/> unit
    /// length and perpendicular to it.</summary>
    public static void OrthoNormalize(ref Vector3 normal, ref Vector3 tangent)
    {
        normal = Normalize(normal);
        tangent = Normalize(tangent - Project(tangent, normal));
        if (tangent == zero) tangent = Normalize(AnyPerpendicular(normal));
    }

    public static void OrthoNormalize(ref Vector3 normal, ref Vector3 tangent, ref Vector3 binormal)
    {
        OrthoNormalize(ref normal, ref tangent);
        binormal = Normalize(binormal - Project(binormal, normal) - Project(binormal, tangent));
        if (binormal == zero) binormal = Cross(normal, tangent);
    }

    /// <summary>Rotates the direction toward the target by at most <paramref name="maxRadiansDelta"/>
    /// and moves the length by at most <paramref name="maxMagnitudeDelta"/>. A negative
    /// angle rotates away until the vectors are opposite.</summary>
    public static Vector3 RotateTowards(Vector3 current, Vector3 target, float maxRadiansDelta, float maxMagnitudeDelta)
    {
        var lengthCurrent = current.magnitude;
        var lengthTarget = target.magnitude;
        if (lengthCurrent < kEpsilon || lengthTarget < kEpsilon) return MoveTowards(current, target, maxMagnitudeDelta);
        var length = Mathf.MoveTowards(lengthCurrent, lengthTarget, maxMagnitudeDelta);
        var from = current / lengthCurrent;
        var to = target / lengthTarget;
        var angle = MathF.Acos(Mathf.Clamp(Dot(from, to), -1f, 1f));
        if (angle < kEpsilon) return to * length;
        var fraction = Mathf.Max(maxRadiansDelta / angle, 1f - MathF.PI / angle);
        return fraction >= 1f ? to * length : RotateDirection(from, to, fraction) * length;
    }

    public static Vector3 RotateTowards(ref Vector3 current, ref Vector3 target, float maxRadiansDelta, float maxMagnitudeDelta) =>
        RotateTowards(current, target, maxRadiansDelta, maxMagnitudeDelta);

    public static Vector3 MoveTowards(Vector3 current, Vector3 target, float maxDistanceDelta)
    {
        var toVector = target - current;
        var distance = toVector.magnitude;
        if (distance <= maxDistanceDelta || distance < kEpsilon) return target;
        return current + toVector / distance * maxDistanceDelta;
    }

    public static Vector3 MoveTowards(ref Vector3 current, ref Vector3 target, float maxDistanceDelta) => MoveTowards(current, target, maxDistanceDelta);

    public static Vector3 SmoothDamp(Vector3 current, Vector3 target, ref Vector3 currentVelocity, float smoothTime, float maxSpeed) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, maxSpeed, Time.deltaTime);

    public static Vector3 SmoothDamp(ref Vector3 current, ref Vector3 target, ref Vector3 currentVelocity, float smoothTime, float maxSpeed) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, maxSpeed, Time.deltaTime);

    public static Vector3 SmoothDamp(Vector3 current, Vector3 target, ref Vector3 currentVelocity, float smoothTime) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, Mathf.Infinity, Time.deltaTime);

    public static Vector3 SmoothDamp(ref Vector3 current, ref Vector3 target, ref Vector3 currentVelocity, float smoothTime) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, Mathf.Infinity, Time.deltaTime);

    public static Vector3 SmoothDamp(ref Vector3 current, ref Vector3 target, ref Vector3 currentVelocity, float smoothTime, float maxSpeed, float deltaTime) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, maxSpeed, deltaTime);

    /// <summary>A critically damped spring toward <paramref name="target"/> that does not
    /// overshoot it.</summary>
    public static Vector3 SmoothDamp(Vector3 current, Vector3 target, ref Vector3 currentVelocity, float smoothTime, float maxSpeed, float deltaTime)
    {
        smoothTime = Mathf.Max(0.0001f, smoothTime);
        var omega = 2f / smoothTime;
        var scaled = omega * deltaTime;
        var decay = 1f / (1f + scaled + 0.48f * scaled * scaled + 0.235f * scaled * scaled * scaled);
        var originalTarget = target;
        var change = ClampMagnitude(current - target, maxSpeed * smoothTime);
        target = current - change;
        var temp = (currentVelocity + omega * change) * deltaTime;
        currentVelocity = (currentVelocity - omega * temp) * decay;
        var output = target + (change + temp) * decay;
        if (Dot(originalTarget - current, output - originalTarget) > 0f)
        {
            output = originalTarget;
            currentVelocity = deltaTime > 0f ? (output - originalTarget) / deltaTime : zero;
        }
        return output;
    }

    public static Vector3 Scale(Vector3 a, Vector3 b) => new(a.x * b.x, a.y * b.y, a.z * b.z);
    public static Vector3 Scale(ref Vector3 a, ref Vector3 b) => Scale(a, b);

    public void Scale(Vector3 scale)
    {
        x *= scale.x;
        y *= scale.y;
        z *= scale.z;
    }

    public void Scale(ref Vector3 scale) => Scale(scale);

    public static Vector3 Cross(Vector3 lhs, Vector3 rhs) =>
        new(lhs.y * rhs.z - lhs.z * rhs.y, lhs.z * rhs.x - lhs.x * rhs.z, lhs.x * rhs.y - lhs.y * rhs.x);

    public static Vector3 Cross(ref Vector3 lhs, ref Vector3 rhs) => Cross(lhs, rhs);

    public override readonly int GetHashCode() => x.GetHashCode() ^ (y.GetHashCode() << 2) ^ (z.GetHashCode() >> 2);
    public override readonly bool Equals(object? other) => other is Vector3 vector && Equals(vector);
    public readonly bool Equals(Vector3 other) => x.Equals(other.x) && y.Equals(other.y) && z.Equals(other.z);
    public readonly bool Equals(ref Vector3 other) => Equals(other);

    public static Vector3 Reflect(Vector3 inDirection, Vector3 inNormal) => inDirection - 2f * Dot(inNormal, inDirection) * inNormal;
    public static Vector3 Reflect(ref Vector3 inDirection, ref Vector3 inNormal) => Reflect(inDirection, inNormal);

    /// <summary>Vectors shorter than <see cref="kEpsilon"/> become zero.</summary>
    public static Vector3 Normalize(Vector3 value)
    {
        var length = value.magnitude;
        return length > kEpsilon ? value / length : zero;
    }

    public static Vector3 Normalize(ref Vector3 value) => Normalize(value);
    public void Normalize() => this = Normalize(this);
    public readonly Vector3 normalized => Normalize(this);

    public static float Dot(Vector3 lhs, Vector3 rhs) => lhs.x * rhs.x + lhs.y * rhs.y + lhs.z * rhs.z;
    public static float Dot(ref Vector3 lhs, ref Vector3 rhs) => Dot(lhs, rhs);

    public static Vector3 Project(Vector3 vector, Vector3 onNormal)
    {
        var squared = Dot(onNormal, onNormal);
        if (squared < Mathf.Epsilon) return zero;
        return onNormal * Dot(vector, onNormal) / squared;
    }

    public static Vector3 Project(ref Vector3 vector, ref Vector3 onNormal) => Project(vector, onNormal);

    public static Vector3 ProjectOnPlane(Vector3 vector, Vector3 planeNormal)
    {
        var squared = Dot(planeNormal, planeNormal);
        if (squared < Mathf.Epsilon) return vector;
        return vector - planeNormal * Dot(vector, planeNormal) / squared;
    }

    public static Vector3 ProjectOnPlane(ref Vector3 vector, ref Vector3 planeNormal) => ProjectOnPlane(vector, planeNormal);

    /// <summary>The unsigned angle in degrees; 0 when either vector is nearly zero.</summary>
    public static float Angle(Vector3 from, Vector3 to)
    {
        var denominator = MathF.Sqrt(from.sqrMagnitude * to.sqrMagnitude);
        if (denominator < kEpsilonNormalSqrt) return 0f;
        var dot = Mathf.Clamp(Dot(from, to) / denominator, -1f, 1f);
        return MathF.Acos(dot) * Mathf.Rad2Deg;
    }

    public static float Angle(ref Vector3 from, ref Vector3 to) => Angle(from, to);

    /// <summary>The angle in degrees, positive when the turn from <paramref name="from"/> to
    /// <paramref name="to"/> is counter-clockwise looking against <paramref name="axis"/>.</summary>
    public static float SignedAngle(Vector3 from, Vector3 to, Vector3 axis) =>
        Angle(from, to) * Mathf.Sign(Dot(axis, Cross(from, to)));

    public static float SignedAngle(ref Vector3 from, ref Vector3 to, ref Vector3 axis) => SignedAngle(from, to, axis);

    public static float Distance(Vector3 a, Vector3 b) => (a - b).magnitude;
    public static float Distance(ref Vector3 a, ref Vector3 b) => Distance(a, b);

    public static Vector3 ClampMagnitude(Vector3 vector, float maxLength)
    {
        var squared = vector.sqrMagnitude;
        if (squared <= maxLength * maxLength) return vector;
        return vector / MathF.Sqrt(squared) * maxLength;
    }

    public static Vector3 ClampMagnitude(ref Vector3 vector, float maxLength) => ClampMagnitude(vector, maxLength);

    public static float Magnitude(Vector3 vector) => vector.magnitude;
    public static float Magnitude(ref Vector3 vector) => vector.magnitude;
    public readonly float magnitude => MathF.Sqrt(x * x + y * y + z * z);
    public static float SqrMagnitude(Vector3 vector) => vector.sqrMagnitude;
    public static float SqrMagnitude(ref Vector3 vector) => vector.sqrMagnitude;
    public readonly float sqrMagnitude => x * x + y * y + z * z;

    public static Vector3 Min(Vector3 lhs, Vector3 rhs) => new(Mathf.Min(lhs.x, rhs.x), Mathf.Min(lhs.y, rhs.y), Mathf.Min(lhs.z, rhs.z));
    public static Vector3 Min(ref Vector3 lhs, ref Vector3 rhs) => Min(lhs, rhs);
    public static Vector3 Max(Vector3 lhs, Vector3 rhs) => new(Mathf.Max(lhs.x, rhs.x), Mathf.Max(lhs.y, rhs.y), Mathf.Max(lhs.z, rhs.z));
    public static Vector3 Max(ref Vector3 lhs, ref Vector3 rhs) => Max(lhs, rhs);

    public static Vector3 zero => new(0f, 0f, 0f);
    public static Vector3 one => new(1f, 1f, 1f);
    public static Vector3 forward => new(0f, 0f, 1f);
    public static Vector3 back => new(0f, 0f, -1f);
    public static Vector3 up => new(0f, 1f, 0f);
    public static Vector3 down => new(0f, -1f, 0f);
    public static Vector3 left => new(-1f, 0f, 0f);
    public static Vector3 right => new(1f, 0f, 0f);
    public static Vector3 positiveInfinity => new(float.PositiveInfinity, float.PositiveInfinity, float.PositiveInfinity);
    public static Vector3 negativeInfinity => new(float.NegativeInfinity, float.NegativeInfinity, float.NegativeInfinity);

    public static Vector3 operator +(Vector3 a, Vector3 b) => new(a.x + b.x, a.y + b.y, a.z + b.z);
    public static Vector3 operator -(Vector3 a, Vector3 b) => new(a.x - b.x, a.y - b.y, a.z - b.z);
    public static Vector3 operator -(Vector3 a) => new(-a.x, -a.y, -a.z);
    public static Vector3 operator *(Vector3 a, float d) => new(a.x * d, a.y * d, a.z * d);
    public static Vector3 operator *(float d, Vector3 a) => new(a.x * d, a.y * d, a.z * d);
    public static Vector3 operator /(Vector3 a, float d) => new(a.x / d, a.y / d, a.z / d);

    public static bool operator ==(Vector3 lhs, Vector3 rhs) => (lhs - rhs).sqrMagnitude < kEpsilon * kEpsilon;
    public static bool operator !=(Vector3 lhs, Vector3 rhs) => !(lhs == rhs);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F2" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"({x.ToString(format, formatProvider)}, {y.ToString(format, formatProvider)}, {z.ToString(format, formatProvider)})";
    }

    public static Vector3 fwd => forward;

    /// <summary>The angle between the vectors in radians. Unity marks this obsolete.</summary>
    public static float AngleBetween(Vector3 from, Vector3 to) =>
        MathF.Acos(Mathf.Clamp(Dot(from.normalized, to.normalized), -1f, 1f));

    /// <summary>Removes the part of <paramref name="fromThat"/> along <paramref name="excludeThis"/>.
    /// Unity marks this obsolete.</summary>
    public static Vector3 Exclude(Vector3 excludeThis, Vector3 fromThat) => fromThat - Project(fromThat, excludeThis);
}
