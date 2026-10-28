namespace UnityEngine;

/// <summary>Unity's random numbers. Unseeded, the sequence differs every run, as in Unity. After
/// <see cref="InitState"/> a seed always gives the same sequence, though not the values Unity's
/// generator gives for that seed. The generator is Marsaglia's xorshift128, whose four words are
/// <see cref="State"/>, so a saved state restores the sequence exactly.</summary>
public static class Random
{
    private const uint Multiplier = 1812433253;

    /// <summary>The generator's four words; setting <see cref="state"/> from a value read earlier repeats the sequence.</summary>
    [Serializable]
    public struct State
    {
        public int s0;
        public int s1;
        public int s2;
        public int s3;
    }

    private static uint _s0, _s1, _s2, _s3;

    static Random() => InitState(Guid.NewGuid().GetHashCode() ^ Environment.TickCount);

    public static void InitState(int seed)
    {
        unchecked
        {
            _s0 = (uint)seed;
            _s1 = _s0 * Multiplier + 1;
            _s2 = _s1 * Multiplier + 1;
            _s3 = _s2 * Multiplier + 1;
        }
    }

    public static State state
    {
        get => new() { s0 = unchecked((int)_s0), s1 = unchecked((int)_s1), s2 = unchecked((int)_s2), s3 = unchecked((int)_s3) };
        set
        {
            if ((value.s0 | value.s1 | value.s2 | value.s3) == 0) throw new ArgumentException("An all-zero state produces only zeros", nameof(value));
            unchecked { (_s0, _s1, _s2, _s3) = ((uint)value.s0, (uint)value.s1, (uint)value.s2, (uint)value.s3); }
        }
    }

    private static uint Next()
    {
        var t = _s0 ^ (_s0 << 11);
        (_s0, _s1, _s2) = (_s1, _s2, _s3);
        _s3 = _s3 ^ (_s3 >> 19) ^ t ^ (t >> 8);
        return _s3;
    }

    /// <summary>A number from 0 to 1, both included.</summary>
    public static float value => (Next() >> 8) / 16777215f;

    public static int Range(int minInclusive, int maxExclusive)
    {
        if (maxExclusive < minInclusive) throw new ArgumentOutOfRangeException(nameof(maxExclusive), "max must not be below min");
        return maxExclusive == minInclusive ? minInclusive : (int)(minInclusive + Next() % (uint)((long)maxExclusive - minInclusive));
    }

    public static float Range(float minInclusive, float maxInclusive) => minInclusive + value * (maxInclusive - minInclusive);

    /// <summary>A point inside or on the unit circle, uniform over its area.</summary>
    public static Vector2 insideUnitCircle
    {
        get
        {
            while (true)
            {
                var point = new Vector2(Range(-1f, 1f), Range(-1f, 1f));
                if (point.sqrMagnitude <= 1f) return point;
            }
        }
    }

    /// <summary>A point inside or on the unit sphere, uniform over its volume.</summary>
    public static Vector3 insideUnitSphere
    {
        get
        {
            while (true)
            {
                var point = new Vector3(Range(-1f, 1f), Range(-1f, 1f), Range(-1f, 1f));
                if (point.sqrMagnitude <= 1f) return point;
            }
        }
    }

    public static Color ColorHSV() => ColorHSV(0f, 1f, 0f, 1f, 0f, 1f, 1f, 1f);
    public static Color ColorHSV(float hueMin, float hueMax) => ColorHSV(hueMin, hueMax, 0f, 1f, 0f, 1f, 1f, 1f);
    public static Color ColorHSV(float hueMin, float hueMax, float saturationMin, float saturationMax) =>
        ColorHSV(hueMin, hueMax, saturationMin, saturationMax, 0f, 1f, 1f, 1f);
    public static Color ColorHSV(float hueMin, float hueMax, float saturationMin, float saturationMax, float valueMin, float valueMax) =>
        ColorHSV(hueMin, hueMax, saturationMin, saturationMax, valueMin, valueMax, 1f, 1f);

    /// <summary>A color whose hue, saturation, value and alpha each lie between the given limits, drawn in that order
    /// with <see cref="value"/> as the blend between the limits.</summary>
    public static Color ColorHSV(float hueMin, float hueMax, float saturationMin, float saturationMax, float valueMin, float valueMax,
        float alphaMin, float alphaMax)
    {
        var hue = Mathf.Lerp(hueMin, hueMax, value);
        var saturation = Mathf.Lerp(saturationMin, saturationMax, value);
        var brightness = Mathf.Lerp(valueMin, valueMax, value);
        var color = Color.HSVToRGB(hue, saturation, brightness, true);
        color.a = Mathf.Lerp(alphaMin, alphaMax, value);
        return color;
    }
}
