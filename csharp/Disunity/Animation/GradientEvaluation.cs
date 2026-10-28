namespace UnityEngine;

/// <summary>Samples a gradient's color and alpha keys. Blend interpolates each channel linearly,
/// Fixed holds the first key at or after the time, and PerceptualBlend interpolates the color in Oklab
/// (treating the key colors as sRGB) and the alpha linearly. Times outside the keys read the end key.</summary>
internal static class GradientEvaluation
{
    public static Color Evaluate(IReadOnlyList<GradientColorKey> colors, IReadOnlyList<GradientAlphaKey> alphas, GradientMode mode, float time)
    {
        time = Mathf.Clamp01(time);
        var rgb = SampleColor(colors, mode, time);
        return new Color(rgb.r, rgb.g, rgb.b, SampleAlpha(alphas, mode, time));
    }

    /// <summary>The index of the first key at or after the time, or the count when none is.</summary>
    private static int Next<T>(IReadOnlyList<T> keys, Func<T, float> timeOf, float time)
    {
        var index = 0;
        while (index < keys.Count && timeOf(keys[index]) < time) index++;
        return index;
    }

    private static Color SampleColor(IReadOnlyList<GradientColorKey> keys, GradientMode mode, float time)
    {
        if (keys.Count == 0) return Color.white;
        var next = Next(keys, key => key.time, time);
        if (next == keys.Count) return keys[^1].color;
        if (mode == GradientMode.Fixed || next == 0) return keys[next].color;
        var (left, right) = (keys[next - 1], keys[next]);
        var span = right.time - left.time;
        var t = span > 0f ? (time - left.time) / span : 1f;
        return mode == GradientMode.PerceptualBlend ? Oklab.Lerp(left.color, right.color, t) : Color.LerpUnclamped(left.color, right.color, t);
    }

    private static float SampleAlpha(IReadOnlyList<GradientAlphaKey> keys, GradientMode mode, float time)
    {
        if (keys.Count == 0) return 1f;
        var next = Next(keys, key => key.time, time);
        if (next == keys.Count) return keys[^1].alpha;
        if (mode == GradientMode.Fixed || next == 0) return keys[next].alpha;
        var (left, right) = (keys[next - 1], keys[next]);
        var span = right.time - left.time;
        return span > 0f ? left.alpha + (right.alpha - left.alpha) * ((time - left.time) / span) : right.alpha;
    }
}

/// <summary>Interpolation in the Oklab color space (Bjorn Ottosson's published matrices).</summary>
internal static class Oklab
{
    public static Color Lerp(Color from, Color to, float t)
    {
        var (a, b) = (ToLab(from), ToLab(to));
        return FromLab(a.L + (b.L - a.L) * t, a.A + (b.A - a.A) * t, a.B + (b.B - a.B) * t);
    }

    private static (float L, float A, float B) ToLab(Color srgb)
    {
        var (r, g, b) = (Mathf.GammaToLinearSpace(srgb.r), Mathf.GammaToLinearSpace(srgb.g), Mathf.GammaToLinearSpace(srgb.b));
        var l = MathF.Cbrt(0.4122214708f * r + 0.5363325363f * g + 0.0514459929f * b);
        var m = MathF.Cbrt(0.2119034982f * r + 0.6806995451f * g + 0.1073969566f * b);
        var s = MathF.Cbrt(0.0883024619f * r + 0.2817188376f * g + 0.6299787005f * b);
        return (0.2104542553f * l + 0.7936177850f * m - 0.0040720468f * s,
            1.9779984951f * l - 2.4285922050f * m + 0.4505937099f * s,
            0.0259040371f * l + 0.7827717662f * m - 0.8086757660f * s);
    }

    private static Color FromLab(float lightness, float a, float b)
    {
        var l = Cube(lightness + 0.3963377774f * a + 0.2158037573f * b);
        var m = Cube(lightness - 0.1055613458f * a - 0.0638541728f * b);
        var s = Cube(lightness - 0.0894841775f * a - 1.2914855480f * b);
        return new Color(
            Mathf.LinearToGammaSpace(4.0767416621f * l - 3.3077115913f * m + 0.2309699292f * s),
            Mathf.LinearToGammaSpace(-1.2684380046f * l + 2.6097574011f * m - 0.3413193965f * s),
            Mathf.LinearToGammaSpace(-0.0041960863f * l - 0.7034186147f * m + 1.7076147010f * s),
            1f);
    }

    private static float Cube(float value) => value * value * value;
}
