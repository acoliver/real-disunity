using UnityEngine;

/// <summary>How a <c>Gradient</c> stores and samples its keys in each mode.</summary>
internal static class GradientTests
{
    public static void Run()
    {
        ANewGradientIsWhite();
        BlendInterpolatesColorAndAlphaSeparately();
        FixedHoldsTheNextKey();
        PerceptualBlendInterpolatesInOklab();
        KeysAreSortedClampedAndLimited();
        EqualityComparesEverything();
        Console.WriteLine("PASS: Gradient blends, steps and stores keys like Unity's");
    }

    private static void ANewGradientIsWhite()
    {
        var gradient = new Gradient();
        Expect.Equal(Color.white, gradient.Evaluate(0.3f), "a new gradient is opaque white");
        Expect.Equal(2, gradient.colorKeyCount, "two color keys");
        Expect.Equal(2, gradient.alphaKeyCount, "two alpha keys");
        Expect.Equal(GradientMode.Blend, gradient.mode, "the default mode is Blend");
    }

    private static Gradient Ramp(GradientMode mode)
    {
        var gradient = new Gradient { mode = mode };
        gradient.SetKeys(
            [new GradientColorKey(Color.red, 0f), new GradientColorKey(Color.blue, 1f)],
            [new GradientAlphaKey(1f, 0f), new GradientAlphaKey(0f, 1f)]);
        return gradient;
    }

    private static void BlendInterpolatesColorAndAlphaSeparately()
    {
        var ramp = Ramp(GradientMode.Blend);
        var quarter = ramp.Evaluate(0.25f);
        Expect.That(Near(quarter, new Color(0.75f, 0f, 0.25f, 0.75f)), $"red to blue and 1 to 0 at a quarter: {quarter}");
        Expect.That(Near(ramp.Evaluate(-1f), new Color(1f, 0f, 0f, 1f)) && Near(ramp.Evaluate(2f), new Color(0f, 0f, 1f, 0f)), "times clamp to [0, 1]");
        var apart = new Gradient();
        apart.SetKeys(
            [new GradientColorKey(Color.black, 0.2f), new GradientColorKey(Color.white, 0.8f)],
            [new GradientAlphaKey(0.2f, 0.5f), new GradientAlphaKey(0.8f, 1f)]);
        Expect.That(Near(apart.Evaluate(0f), new Color(0f, 0f, 0f, 0.2f)), "before the first keys each channel holds its first key");
        Expect.That(Near(apart.Evaluate(0.5f), new Color(0.5f, 0.5f, 0.5f, 0.2f)), "colors and alphas use their own key times");
        Expect.That(Near(apart.Evaluate(0.75f), new Color(0.916667f, 0.916667f, 0.916667f, 0.5f)), "midway through the alpha span");
        Expect.That(Near(apart.Evaluate(1f), new Color(1f, 1f, 1f, 0.8f)), "after the last color key");
    }

    private static void FixedHoldsTheNextKey()
    {
        var gradient = new Gradient { mode = GradientMode.Fixed };
        gradient.SetKeys(
            [new GradientColorKey(Color.red, 0.25f), new GradientColorKey(Color.green, 0.5f), new GradientColorKey(Color.blue, 1f)],
            [new GradientAlphaKey(1f, 0.5f), new GradientAlphaKey(0.25f, 1f)]);
        Expect.Equal(Color.red, Opaque(gradient.Evaluate(0f)), "before the first key reads the first key");
        Expect.Equal(Color.red, Opaque(gradient.Evaluate(0.25f)), "at a key reads that key");
        Expect.Equal(Color.green, Opaque(gradient.Evaluate(0.3f)), "after a key reads the next key");
        Expect.Equal(Color.green, Opaque(gradient.Evaluate(0.5f)), "at the second key");
        Expect.Equal(Color.blue, Opaque(gradient.Evaluate(0.51f)), "between the second and third keys");
        Expect.Equal(Color.blue, Opaque(gradient.Evaluate(1f)), "at the end");
        Expect.Equal(1f, gradient.Evaluate(0.4f).a, "alpha steps with its own keys");
        Expect.Equal(0.25f, gradient.Evaluate(0.6f).a, "alpha after the first alpha key");
    }

    private static void PerceptualBlendInterpolatesInOklab()
    {
        var gradient = new Gradient { mode = GradientMode.PerceptualBlend };
        gradient.SetKeys(
            [new GradientColorKey(Color.black, 0f), new GradientColorKey(Color.white, 1f)],
            [new GradientAlphaKey(1f, 0f), new GradientAlphaKey(0f, 1f)]);
        // Oklab lightness 0.5 is linear light 0.125, which encodes to 0.3886 in sRGB.
        var middle = gradient.Evaluate(0.5f);
        Expect.Near(0.3886f, middle.r, "the Oklab midpoint of black and white", 2e-3f);
        Expect.That(Near(new Color(middle.g, middle.b, 0f, 0f), new Color(middle.r, middle.r, 0f, 0f), 2e-3f), "grays stay gray");
        Expect.Near(0.5f, middle.a, "alpha blends linearly");
        Expect.That(Near(gradient.Evaluate(0f), new Color(0f, 0f, 0f, 1f), 1e-3f)
            && Near(gradient.Evaluate(1f), new Color(1f, 1f, 1f, 0f), 1e-3f), "the ends are the keys");
        var keys = new[] { new GradientColorKey(Color.red, 0f), new GradientColorKey(Color.green, 1f) };
        var perceptual = new Gradient { mode = GradientMode.PerceptualBlend, colorKeys = keys };
        var blend = new Gradient { colorKeys = keys };
        Expect.That(!Near(perceptual.Evaluate(0.5f), blend.Evaluate(0.5f), 1e-2f), "Oklab and linear blending give different midpoints for red and green");
    }

    private static void KeysAreSortedClampedAndLimited()
    {
        var gradient = new Gradient();
        gradient.colorKeys = [new GradientColorKey(Color.blue, 2f), new GradientColorKey(Color.red, -1f), new GradientColorKey(Color.green, 0.5f)];
        Expect.Sequence(new[] { 0f, 0.5f, 1f }, gradient.colorKeys.Select(key => key.time), "color key times are clamped and sorted");
        Expect.Equal(Color.red, gradient.colorKeys[0].color, "the earliest key leads");
        var copy = gradient.colorKeys;
        copy[0].color = Color.black;
        Expect.Equal(Color.red, gradient.colorKeys[0].color, "colorKeys returns a copy");
        gradient.alphaKeys = [new GradientAlphaKey(0.3f, 0.7f), new GradientAlphaKey(0.9f, 0.1f)];
        Expect.That(gradient.alphaKeys[0].alpha == 0.9f && gradient.alphaKeys[1].time == 0.7f, "alpha keys are sorted");
        var many = Enumerable.Range(0, 12).Select(i => new GradientAlphaKey(i / 12f, i / 12f)).ToArray();
        gradient.alphaKeys = many;
        Expect.Equal(8, gradient.alphaKeyCount, "alpha keys past eight are dropped");
        Expect.Equal(7f / 12f, gradient.alphaKeys[7].alpha, "the first eight are kept");
        var span = new GradientAlphaKey[8];
        gradient.GetAlphaKeys(span);
        Expect.Equal(many[3].alpha, span[3].alpha, "GetAlphaKeys fills the span");
        var colors = new GradientColorKey[3];
        gradient.GetColorKeys(colors);
        Expect.Equal(Color.green, colors[1].color, "GetColorKeys fills the span");
        Expect.Throws<ArgumentException>(() => gradient.GetColorKeys(new GradientColorKey[1]), "a short span");
        var byRef = Color.cyan;
        Expect.Equal(Color.cyan, new GradientColorKey(ref byRef, 0.1f).color, "the ref constructor");
    }

    private static void EqualityComparesEverything()
    {
        var a = Ramp(GradientMode.Blend);
        var b = Ramp(GradientMode.Blend);
        Expect.That(a.Equals(b) && a.Equals((object)b) && a.GetHashCode() == b.GetHashCode(), "gradients with the same keys are equal");
        b.mode = GradientMode.Fixed;
        Expect.That(!a.Equals(b), "mode differs");
        b.mode = GradientMode.Blend;
        b.colorSpace = ColorSpace.Linear;
        Expect.That(!a.Equals(b), "color space differs");
        b.colorSpace = a.colorSpace;
        b.alphaKeys = [new GradientAlphaKey(1f, 0f), new GradientAlphaKey(0.5f, 1f)];
        Expect.That(!a.Equals(b), "alpha keys differ");
        Expect.That(!a.Equals((Gradient?)null) && !a.Equals("x"), "no gradient equals null or another type");
    }

    private static Color Opaque(Color color) => new(color.r, color.g, color.b, 1f);

    private static bool Near(Color a, Color b, float tolerance = 1e-4f) =>
        MathF.Abs(a.r - b.r) <= tolerance && MathF.Abs(a.g - b.g) <= tolerance
        && MathF.Abs(a.b - b.b) <= tolerance && MathF.Abs(a.a - b.a) <= tolerance;
}
