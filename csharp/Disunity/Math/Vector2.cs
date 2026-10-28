using System.Globalization;

namespace UnityEngine;

/// <summary>A 2D vector of two floats, laid out as the engine's native code expects.
/// <c>==</c> is approximate: two vectors are equal when the squared length of their
/// difference is below <see cref="kEpsilon"/> squared. <c>Equals</c> compares exactly.</summary>
public struct Vector2 : IEquatable<Vector2>, IFormattable
{
    public const float kEpsilon = 0.00001f;
    public const float kEpsilonNormalSqrt = 1e-15f;

    public float x;
    public float y;

    public Vector2(float x, float y)
    {
        this.x = x;
        this.y = y;
    }

    public float this[int index]
    {
        readonly get => index switch
        {
            0 => x,
            1 => y,
            _ => throw new IndexOutOfRangeException("Invalid Vector2 index!"),
        };
        set
        {
            switch (index)
            {
                case 0: x = value; break;
                case 1: y = value; break;
                default: throw new IndexOutOfRangeException("Invalid Vector2 index!");
            }
        }
    }

    public void Set(float newX, float newY)
    {
        x = newX;
        y = newY;
    }

    public static Vector2 Lerp(Vector2 a, Vector2 b, float t) => LerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Vector2 Lerp(ref Vector2 a, ref Vector2 b, float t) => Lerp(a, b, t);
    public static Vector2 LerpUnclamped(Vector2 a, Vector2 b, float t) => new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
    public static Vector2 LerpUnclamped(ref Vector2 a, ref Vector2 b, float t) => LerpUnclamped(a, b, t);

    public static Vector2 MoveTowards(Vector2 current, Vector2 target, float maxDistanceDelta)
    {
        var toVector = target - current;
        var distance = toVector.magnitude;
        if (distance <= maxDistanceDelta || distance == 0f) return target;
        return current + toVector / distance * maxDistanceDelta;
    }

    public static Vector2 MoveTowards(ref Vector2 current, ref Vector2 target, float maxDistanceDelta) => MoveTowards(current, target, maxDistanceDelta);

    public static Vector2 Scale(Vector2 a, Vector2 b) => new(a.x * b.x, a.y * b.y);
    public static Vector2 Scale(ref Vector2 a, ref Vector2 b) => Scale(a, b);

    public void Scale(Vector2 scale)
    {
        x *= scale.x;
        y *= scale.y;
    }

    public void Scale(ref Vector2 scale) => Scale(scale);

    /// <summary>Vectors shorter than <see cref="kEpsilon"/> become zero.</summary>
    public static Vector2 Normalize(Vector2 value)
    {
        var length = value.magnitude;
        return length > kEpsilon ? value / length : zero;
    }

    public static Vector2 Normalize(ref Vector2 value) => Normalize(value);
    public void Normalize() => this = Normalize(this);
    public readonly Vector2 normalized => Normalize(this);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F2" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"({x.ToString(format, formatProvider)}, {y.ToString(format, formatProvider)})";
    }

    public override readonly int GetHashCode() => x.GetHashCode() ^ (y.GetHashCode() << 2);
    public override readonly bool Equals(object? other) => other is Vector2 vector && Equals(vector);
    public readonly bool Equals(Vector2 other) => x.Equals(other.x) && y.Equals(other.y);
    public readonly bool Equals(ref Vector2 other) => Equals(other);

    public static Vector2 Reflect(Vector2 inDirection, Vector2 inNormal) => inDirection - 2f * Dot(inNormal, inDirection) * inNormal;
    public static Vector2 Reflect(ref Vector2 inDirection, ref Vector2 inNormal) => Reflect(inDirection, inNormal);

    /// <summary>The vector rotated 90 degrees counter-clockwise.</summary>
    public static Vector2 Perpendicular(Vector2 inDirection) => new(-inDirection.y, inDirection.x);
    public static Vector2 Perpendicular(ref Vector2 inDirection) => Perpendicular(inDirection);

    public static float Dot(Vector2 lhs, Vector2 rhs) => lhs.x * rhs.x + lhs.y * rhs.y;
    public static float Dot(ref Vector2 lhs, ref Vector2 rhs) => Dot(lhs, rhs);

    public readonly float magnitude => MathF.Sqrt(x * x + y * y);
    public readonly float sqrMagnitude => x * x + y * y;

    /// <summary>The unsigned angle in degrees; 0 when either vector is nearly zero.</summary>
    public static float Angle(Vector2 from, Vector2 to)
    {
        var denominator = MathF.Sqrt(from.sqrMagnitude * to.sqrMagnitude);
        if (denominator < kEpsilonNormalSqrt) return 0f;
        var dot = Mathf.Clamp(Dot(from, to) / denominator, -1f, 1f);
        return MathF.Acos(dot) * Mathf.Rad2Deg;
    }

    public static float Angle(ref Vector2 from, ref Vector2 to) => Angle(from, to);

    /// <summary>Positive when <paramref name="to"/> is counter-clockwise from <paramref name="from"/>.</summary>
    public static float SignedAngle(Vector2 from, Vector2 to) => Angle(from, to) * Mathf.Sign(from.x * to.y - from.y * to.x);
    public static float SignedAngle(ref Vector2 from, ref Vector2 to) => SignedAngle(from, to);

    public static float Distance(Vector2 a, Vector2 b) => (a - b).magnitude;
    public static float Distance(ref Vector2 a, ref Vector2 b) => Distance(a, b);

    public static Vector2 ClampMagnitude(Vector2 vector, float maxLength)
    {
        var squared = vector.sqrMagnitude;
        if (squared <= maxLength * maxLength) return vector;
        return vector / MathF.Sqrt(squared) * maxLength;
    }

    public static Vector2 ClampMagnitude(ref Vector2 vector, float maxLength) => ClampMagnitude(vector, maxLength);

    public static float SqrMagnitude(Vector2 a) => a.x * a.x + a.y * a.y;
    public static float SqrMagnitude(ref Vector2 a) => SqrMagnitude(a);
    public readonly float SqrMagnitude() => x * x + y * y;

    public static Vector2 Min(Vector2 lhs, Vector2 rhs) => new(Mathf.Min(lhs.x, rhs.x), Mathf.Min(lhs.y, rhs.y));
    public static Vector2 Min(ref Vector2 lhs, ref Vector2 rhs) => Min(lhs, rhs);
    public static Vector2 Max(Vector2 lhs, Vector2 rhs) => new(Mathf.Max(lhs.x, rhs.x), Mathf.Max(lhs.y, rhs.y));
    public static Vector2 Max(ref Vector2 lhs, ref Vector2 rhs) => Max(lhs, rhs);

    public static Vector2 SmoothDamp(Vector2 current, Vector2 target, ref Vector2 currentVelocity, float smoothTime, float maxSpeed) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, maxSpeed, Time.deltaTime);

    public static Vector2 SmoothDamp(ref Vector2 current, ref Vector2 target, ref Vector2 currentVelocity, float smoothTime, float maxSpeed) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, maxSpeed, Time.deltaTime);

    public static Vector2 SmoothDamp(Vector2 current, Vector2 target, ref Vector2 currentVelocity, float smoothTime) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, Mathf.Infinity, Time.deltaTime);

    public static Vector2 SmoothDamp(ref Vector2 current, ref Vector2 target, ref Vector2 currentVelocity, float smoothTime) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, Mathf.Infinity, Time.deltaTime);

    public static Vector2 SmoothDamp(ref Vector2 current, ref Vector2 target, ref Vector2 currentVelocity, float smoothTime, float maxSpeed, float deltaTime) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, maxSpeed, deltaTime);

    /// <summary>A critically damped spring toward <paramref name="target"/> that does not
    /// overshoot it.</summary>
    public static Vector2 SmoothDamp(Vector2 current, Vector2 target, ref Vector2 currentVelocity, float smoothTime, float maxSpeed, float deltaTime)
    {
        smoothTime = Mathf.Max(0.0001f, smoothTime);
        var omega = 2f / smoothTime;
        var x = omega * deltaTime;
        var decay = 1f / (1f + x + 0.48f * x * x + 0.235f * x * x * x);
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

    public static Vector2 operator +(Vector2 a, Vector2 b) => new(a.x + b.x, a.y + b.y);
    public static Vector2 operator -(Vector2 a, Vector2 b) => new(a.x - b.x, a.y - b.y);
    public static Vector2 operator *(Vector2 a, Vector2 b) => new(a.x * b.x, a.y * b.y);
    public static Vector2 operator /(Vector2 a, Vector2 b) => new(a.x / b.x, a.y / b.y);
    public static Vector2 operator -(Vector2 a) => new(-a.x, -a.y);
    public static Vector2 operator *(Vector2 a, float d) => new(a.x * d, a.y * d);
    public static Vector2 operator *(float d, Vector2 a) => new(a.x * d, a.y * d);
    public static Vector2 operator /(Vector2 a, float d) => new(a.x / d, a.y / d);

    public static bool operator ==(Vector2 lhs, Vector2 rhs) => SqrMagnitude(lhs - rhs) < kEpsilon * kEpsilon;
    public static bool operator !=(Vector2 lhs, Vector2 rhs) => !(lhs == rhs);

    public static implicit operator Vector2(Vector3 v) => new(v.x, v.y);
    public static implicit operator Vector3(Vector2 v) => new(v.x, v.y, 0f);

    public static Vector2 zero => new(0f, 0f);
    public static Vector2 one => new(1f, 1f);
    public static Vector2 up => new(0f, 1f);
    public static Vector2 down => new(0f, -1f);
    public static Vector2 left => new(-1f, 0f);
    public static Vector2 right => new(1f, 0f);
    public static Vector2 positiveInfinity => new(float.PositiveInfinity, float.PositiveInfinity);
    public static Vector2 negativeInfinity => new(float.NegativeInfinity, float.NegativeInfinity);
}
