namespace UnityEngine;

/// <summary>A color at a position in [0, 1] along a <see cref="Gradient"/>. The color's alpha is ignored;
/// alpha comes from the gradient's <see cref="GradientAlphaKey"/> values.</summary>
public struct GradientColorKey
{
    public Color color;
    public float time;

    public GradientColorKey(Color col, float time)
    {
        color = col;
        this.time = time;
    }

    public GradientColorKey(ref Color col, float time) : this(col, time) { }
}

/// <summary>An alpha at a position in [0, 1] along a <see cref="Gradient"/>.</summary>
public struct GradientAlphaKey
{
    public float alpha;
    public float time;

    public GradientAlphaKey(float alpha, float time)
    {
        this.alpha = alpha;
        this.time = time;
    }
}
