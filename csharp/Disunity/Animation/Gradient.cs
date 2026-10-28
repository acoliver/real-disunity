namespace UnityEngine;

/// <summary>Up to eight color keys and eight alpha keys that blend, step or perceptually blend along
/// [0, 1]. A new gradient is white and opaque. Keys beyond eight are dropped with a warning, key times
/// are clamped to [0, 1], and keys are kept sorted by time. Equality compares keys, mode and color space.</summary>
public class Gradient : IEquatable<Gradient>
{
    internal const int MaxKeys = 8;

    private readonly List<GradientColorKey> colors = [];
    private readonly List<GradientAlphaKey> alphas = [];

    public Gradient()
    {
        colors.Add(new GradientColorKey(Color.white, 0f));
        colors.Add(new GradientColorKey(Color.white, 1f));
        alphas.Add(new GradientAlphaKey(1f, 0f));
        alphas.Add(new GradientAlphaKey(1f, 1f));
    }

    public GradientMode mode { get; set; }

    /// <summary>Stored; evaluation does not convert between color spaces.</summary>
    public ColorSpace colorSpace { get; set; }

    public GradientColorKey[] colorKeys
    {
        get => colors.ToArray();
        set => SetColorKeys(value);
    }

    public GradientAlphaKey[] alphaKeys
    {
        get => alphas.ToArray();
        set => SetAlphaKeys(value);
    }

    public int colorKeyCount => colors.Count;

    public int alphaKeyCount => alphas.Count;

    public Color Evaluate(float time) => GradientEvaluation.Evaluate(colors, alphas, mode, time);

    public void GetColorKeys(Span<GradientColorKey> keys) => CopyOut(colors, keys);

    public void GetAlphaKeys(Span<GradientAlphaKey> keys) => CopyOut(alphas, keys);

    public void SetColorKeys(ReadOnlySpan<GradientColorKey> keys)
    {
        Replace(colors, keys, key => key.time, (key, time) => { key.time = time; return key; });
    }

    public void SetAlphaKeys(ReadOnlySpan<GradientAlphaKey> keys)
    {
        Replace(alphas, keys, key => key.time, (key, time) => { key.time = time; return key; });
    }

    public void SetKeys(GradientColorKey[] colorKeys, GradientAlphaKey[] alphaKeys)
    {
        SetColorKeys(colorKeys);
        SetAlphaKeys(alphaKeys);
    }

    public void SetKeys(ReadOnlySpan<GradientColorKey> colorKeys, ReadOnlySpan<GradientAlphaKey> alphaKeys)
    {
        SetColorKeys(colorKeys);
        SetAlphaKeys(alphaKeys);
    }

    public override bool Equals(object? o) => o is Gradient other && Equals(other);

    public bool Equals(Gradient? other) =>
        other is not null
        && (ReferenceEquals(this, other)
            || (mode == other.mode && colorSpace == other.colorSpace
                && colors.Count == other.colors.Count && colors.Zip(other.colors).All(p => p.First.color.Equals(p.Second.color) && p.First.time.Equals(p.Second.time))
                && alphas.Count == other.alphas.Count && alphas.Zip(other.alphas).All(p => p.First.alpha.Equals(p.Second.alpha) && p.First.time.Equals(p.Second.time))));

    public override int GetHashCode()
    {
        var hash = new HashCode();
        hash.Add(mode);
        hash.Add(colorSpace);
        foreach (var key in colors) hash.Add((key.color, key.time));
        foreach (var key in alphas) hash.Add((key.alpha, key.time));
        return hash.ToHashCode();
    }

    private static void CopyOut<T>(List<T> keys, Span<T> destination)
    {
        if (destination.Length < keys.Count) throw new ArgumentException("The span is shorter than the number of keys.", nameof(destination));
        for (var i = 0; i < keys.Count; i++) destination[i] = keys[i];
    }

    private static void Replace<T>(List<T> target, ReadOnlySpan<T> keys, Func<T, float> timeOf, Func<T, float, T> withTime)
    {
        if (keys.Length > MaxKeys) Debug.LogWarning($"Gradient can only have {MaxKeys} keys of each kind; the extra keys are ignored.");
        var kept = new List<T>();
        foreach (var key in keys[..Math.Min(keys.Length, MaxKeys)])
            kept.Add(withTime(key, Mathf.Clamp01(timeOf(key))));
        target.Clear();
        target.AddRange(kept.OrderBy(timeOf));
    }
}
