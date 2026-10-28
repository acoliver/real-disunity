namespace UnityEngine;

/// <summary>Unity's collection of math functions on floats. Unity declares it as a struct
/// holding only static members, and this type matches so that player assemblies bind to it.</summary>
public struct Mathf
{
    public const float PI = 3.1415927f;
    public const float Infinity = float.PositiveInfinity;
    public const float NegativeInfinity = float.NegativeInfinity;
    public const float Deg2Rad = PI * 2f / 360f;
    public const float Rad2Deg = 1f / Deg2Rad;
    public static readonly float Epsilon = float.Epsilon;

    public static float Sin(float f) => MathF.Sin(f);
    public static float Cos(float f) => MathF.Cos(f);
    public static float Tan(float f) => MathF.Tan(f);
    public static float Asin(float f) => MathF.Asin(f);
    public static float Acos(float f) => MathF.Acos(f);
    public static float Atan(float f) => MathF.Atan(f);
    public static float Atan2(float y, float x) => MathF.Atan2(y, x);
    public static float Sqrt(float f) => MathF.Sqrt(f);
    public static float Abs(float f) => MathF.Abs(f);
    public static int Abs(int value) => Math.Abs(value);
    public static float Pow(float f, float p) => MathF.Pow(f, p);
    public static float Exp(float power) => MathF.Exp(power);
    public static float Log(float f, float p) => MathF.Log(f, p);
    public static float Log(float f) => MathF.Log(f);
    public static float Log10(float f) => MathF.Log10(f);
    public static float Ceil(float f) => MathF.Ceiling(f);
    public static float Floor(float f) => MathF.Floor(f);

    /// <summary>Rounds half to even, so 0.5 gives 0 and 1.5 gives 2.</summary>
    public static float Round(float f) => MathF.Round(f);
    public static int CeilToInt(float f) => (int)MathF.Ceiling(f);
    public static int FloorToInt(float f) => (int)MathF.Floor(f);
    public static int RoundToInt(float f) => (int)MathF.Round(f);

    /// <summary>Returns 1 for zero, unlike <see cref="Math.Sign(float)"/>.</summary>
    public static float Sign(float f) => f >= 0f ? 1f : -1f;

    public static float Min(float a, float b) => a < b ? a : b;
    public static int Min(int a, int b) => a < b ? a : b;
    public static float Max(float a, float b) => a > b ? a : b;
    public static int Max(int a, int b) => a > b ? a : b;
    public static float Min(params float[] values) => Fold(values, Min);
    public static int Min(params int[] values) => Fold(values, Min);
    public static float Max(params float[] values) => Fold(values, Max);
    public static int Max(params int[] values) => Fold(values, Max);

    private static T Fold<T>(T[] values, Func<T, T, T> combine) where T : struct
    {
        if (values.Length == 0) return default;
        var result = values[0];
        for (var i = 1; i < values.Length; i++) result = combine(result, values[i]);
        return result;
    }

    /// <summary>Does not throw when <paramref name="min"/> is above <paramref name="max"/>;
    /// the minimum wins for values below it, then the maximum for values above it.</summary>
    public static float Clamp(float value, float min, float max)
    {
        if (value < min) return min;
        return value > max ? max : value;
    }

    public static int Clamp(int value, int min, int max)
    {
        if (value < min) return min;
        return value > max ? max : value;
    }

    public static float Clamp01(float value)
    {
        if (value < 0f) return 0f;
        return value > 1f ? 1f : value;
    }

    public static float Lerp(float a, float b, float t) => a + (b - a) * Clamp01(t);
    public static float LerpUnclamped(float a, float b, float t) => a + (b - a) * t;

    public static float LerpAngle(float a, float b, float t)
    {
        var delta = Repeat(b - a, 360f);
        if (delta > 180f) delta -= 360f;
        return a + delta * Clamp01(t);
    }

    public static float MoveTowards(float current, float target, float maxDelta)
    {
        if (Abs(target - current) <= maxDelta) return target;
        return current + Sign(target - current) * maxDelta;
    }

    public static float MoveTowardsAngle(float current, float target, float maxDelta)
    {
        var delta = DeltaAngle(current, target);
        if (-maxDelta < delta && delta < maxDelta) return target;
        return MoveTowards(current, current + delta, maxDelta);
    }

    public static float SmoothStep(float from, float to, float t)
    {
        t = Clamp01(t);
        t = -2f * t * t * t + 3f * t * t;
        return to * t + from * (1f - t);
    }

    public static float Gamma(float value, float absmax, float gamma)
    {
        var magnitude = Abs(value);
        if (magnitude > absmax) return value < 0f ? -magnitude : magnitude;
        var result = Pow(magnitude / absmax, gamma) * absmax;
        return value < 0f ? -result : result;
    }

    /// <summary>True when the difference is below one millionth of the larger magnitude, or
    /// below eight times <see cref="Epsilon"/> for values near zero.</summary>
    public static bool Approximately(float a, float b) =>
        Abs(b - a) < Max(0.000001f * Max(Abs(a), Abs(b)), Epsilon * 8f);

    public static float SmoothDamp(float current, float target, ref float currentVelocity, float smoothTime, float maxSpeed) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, maxSpeed, Time.deltaTime);

    public static float SmoothDamp(float current, float target, ref float currentVelocity, float smoothTime) =>
        SmoothDamp(current, target, ref currentVelocity, smoothTime, Infinity, Time.deltaTime);

    /// <summary>A critically damped spring toward <paramref name="target"/> that does not
    /// overshoot it.</summary>
    public static float SmoothDamp(float current, float target, ref float currentVelocity, float smoothTime, float maxSpeed, float deltaTime)
    {
        smoothTime = Max(0.0001f, smoothTime);
        var omega = 2f / smoothTime;
        var x = omega * deltaTime;
        var decay = 1f / (1f + x + 0.48f * x * x + 0.235f * x * x * x);
        var originalTarget = target;
        var maxChange = maxSpeed * smoothTime;
        var change = Clamp(current - target, -maxChange, maxChange);
        target = current - change;
        var temp = (currentVelocity + omega * change) * deltaTime;
        currentVelocity = (currentVelocity - omega * temp) * decay;
        var output = target + (change + temp) * decay;
        if ((originalTarget - current > 0f) == (output > originalTarget))
        {
            output = originalTarget;
            currentVelocity = deltaTime > 0f ? (output - originalTarget) / deltaTime : 0f;
        }
        return output;
    }

    public static float SmoothDampAngle(float current, float target, ref float currentVelocity, float smoothTime, float maxSpeed) =>
        SmoothDampAngle(current, target, ref currentVelocity, smoothTime, maxSpeed, Time.deltaTime);

    public static float SmoothDampAngle(float current, float target, ref float currentVelocity, float smoothTime) =>
        SmoothDampAngle(current, target, ref currentVelocity, smoothTime, Infinity, Time.deltaTime);

    public static float SmoothDampAngle(float current, float target, ref float currentVelocity, float smoothTime, float maxSpeed, float deltaTime) =>
        SmoothDamp(current, current + DeltaAngle(current, target), ref currentVelocity, smoothTime, maxSpeed, deltaTime);

    /// <summary>Wraps <paramref name="t"/> into [0, length), also for negative values.</summary>
    public static float Repeat(float t, float length) => Clamp(t - Floor(t / length) * length, 0f, length);

    public static float PingPong(float t, float length)
    {
        t = Repeat(t, length * 2f);
        return length - Abs(t - length);
    }

    /// <summary>Returns 0 when <paramref name="a"/> equals <paramref name="b"/>.</summary>
    public static float InverseLerp(float a, float b, float value) => a != b ? Clamp01((value - a) / (b - a)) : 0f;

    /// <summary>The shortest signed difference in degrees, in [-180, 180].</summary>
    public static float DeltaAngle(float current, float target)
    {
        var delta = Repeat(target - current, 360f);
        return delta > 180f ? delta - 360f : delta;
    }

    /// <summary>The smallest power of two that is at least <paramref name="value"/>; 0 for
    /// values of 0 or less.</summary>
    public static int NextPowerOfTwo(int value)
    {
        if (value <= 0) return 0;
        return (int)System.Numerics.BitOperations.RoundUpToPowerOf2((uint)value);
    }

    /// <summary>The nearest power of two; a value halfway between two powers goes up.</summary>
    public static int ClosestPowerOfTwo(int value)
    {
        var next = NextPowerOfTwo(value);
        var previous = next >> 1;
        return value - previous < next - value ? previous : next;
    }

    public static bool IsPowerOfTwo(int value) => (value & (value - 1)) == 0;

    public static float GammaToLinearSpace(float value) =>
        value <= 0.04045f ? value / 12.92f : MathF.Pow((value + 0.055f) / 1.055f, 2.4f);

    public static float LinearToGammaSpace(float value) =>
        value <= 0.0031308f ? value * 12.92f : 1.055f * MathF.Pow(value, 1f / 2.4f) - 0.055f;

    public static ushort FloatToHalf(float val) => BitConverter.HalfToUInt16Bits((Half)val);
    public static float HalfToFloat(ushort val) => (float)BitConverter.UInt16BitsToHalf(val);

    /// <summary>An approximation of the black body color at <paramref name="kelvin"/>, a
    /// curve fit to published black body data, clamped to the range 1000 to 40000 K. It does
    /// not reproduce Unity's table values exactly.</summary>
    public static Color CorrelatedColorTemperatureToRGB(float kelvin)
    {
        var t = Clamp(kelvin, 1000f, 40000f) / 100f;
        var r = t <= 66f ? 255f : 329.698727446f * Pow(t - 60f, -0.1332047592f);
        var g = t <= 66f ? 99.4708025861f * Log(t) - 161.1195681661f : 288.1221695283f * Pow(t - 60f, -0.0755148492f);
        var b = t >= 66f ? 255f : t <= 19f ? 0f : 138.5177312231f * Log(t - 10f) - 305.0447927307f;
        return new Color(Clamp(r, 0f, 255f) / 255f, Clamp(g, 0f, 255f) / 255f, Clamp(b, 0f, 255f) / 255f, 1f);
    }

    /// <summary>Gradient noise in [0, 1] that is 0.5 at integer coordinates. It is improved
    /// Perlin noise with a fixed permutation, so it has Unity's character but not Unity's
    /// exact values.</summary>
    public static float PerlinNoise(float x, float y) => PerlinNoiseTable.Noise2D(x, y);

    /// <summary>One dimensional gradient noise in [0, 1]; see <see cref="PerlinNoise"/> for
    /// how it differs from Unity.</summary>
    public static float PerlinNoise1D(float x) => PerlinNoiseTable.Noise1D(x);
}
