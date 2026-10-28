namespace UnityEngine;

internal static class PerlinNoiseTable
{
    private static readonly byte[] Permutation = BuildPermutation();

    private static byte[] BuildPermutation()
    {
        var table = new byte[256];
        for (var i = 0; i < 256; i++) table[i] = (byte)i;
        uint state = 0x9E3779B9u;
        for (var i = 255; i > 0; i--)
        {
            state = state * 1664525u + 1013904223u;
            var j = (int)((state >> 8) % (uint)(i + 1));
            (table[i], table[j]) = (table[j], table[i]);
        }
        return table;
    }

    private static float Fade(float t) => t * t * t * (t * (t * 6f - 15f) + 10f);
    private static float Mix(float a, float b, float t) => a + (b - a) * t;
    private static int Hash(int a, int b) => Permutation[(Permutation[a & 255] + b) & 255];

    private static float Gradient(int hash, float x, float y) => (hash & 7) switch
    {
        0 => x,
        1 => -x,
        2 => y,
        3 => -y,
        4 => (x + y) * 0.70710678f,
        5 => (x - y) * 0.70710678f,
        6 => (-x + y) * 0.70710678f,
        _ => (-x - y) * 0.70710678f,
    };

    public static float Noise2D(float x, float y)
    {
        if (!float.IsFinite(x) || !float.IsFinite(y)) return 0.5f;
        var fx = MathF.Floor(x);
        var fy = MathF.Floor(y);
        int xi = (int)fx, yi = (int)fy;
        float dx = x - fx, dy = y - fy;
        var u = Fade(dx);
        var v = Fade(dy);
        var bottom = Mix(Gradient(Hash(xi, yi), dx, dy), Gradient(Hash(xi + 1, yi), dx - 1f, dy), u);
        var top = Mix(Gradient(Hash(xi, yi + 1), dx, dy - 1f), Gradient(Hash(xi + 1, yi + 1), dx - 1f, dy - 1f), u);
        return Mathf.Clamp01(Mix(bottom, top, v) * 0.5f * 1.4142135f + 0.5f);
    }

    public static float Noise1D(float x)
    {
        if (!float.IsFinite(x)) return 0.5f;
        var fx = MathF.Floor(x);
        var xi = (int)fx;
        var dx = x - fx;
        var left = (Permutation[xi & 255] & 1) == 0 ? dx : -dx;
        var right = (Permutation[(xi + 1) & 255] & 1) == 0 ? dx - 1f : 1f - dx;
        return Mathf.Clamp01(Mix(left, right, Fade(dx)) * 1.6f + 0.5f);
    }
}
