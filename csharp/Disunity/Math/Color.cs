using System.Globalization;

namespace UnityEngine;

/// <summary>An RGBA color of four floats, normally in [0, 1]. <c>==</c> compares the colors as
/// <see cref="Vector4"/> values, so it is approximate; <c>Equals</c> compares exactly.</summary>
public struct Color : IEquatable<Color>, IFormattable
{
    public float r;
    public float g;
    public float b;
    public float a;

    public Color(float r, float g, float b, float a)
    {
        this.r = r;
        this.g = g;
        this.b = b;
        this.a = a;
    }

    public Color(float r, float g, float b) : this(r, g, b, 1f) { }

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        format = string.IsNullOrEmpty(format) ? "F3" : format;
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"RGBA({r.ToString(format, formatProvider)}, {g.ToString(format, formatProvider)}, {b.ToString(format, formatProvider)}, {a.ToString(format, formatProvider)})";
    }

    public override readonly int GetHashCode() => ((Vector4)this).GetHashCode();
    public override readonly bool Equals(object? other) => other is Color color && Equals(color);
    public readonly bool Equals(Color other) => r.Equals(other.r) && g.Equals(other.g) && b.Equals(other.b) && a.Equals(other.a);
    public readonly bool Equals(ref Color other) => Equals(other);

    public static Color operator +(Color a, Color b) => new(a.r + b.r, a.g + b.g, a.b + b.b, a.a + b.a);
    public static Color operator -(Color a, Color b) => new(a.r - b.r, a.g - b.g, a.b - b.b, a.a - b.a);
    public static Color operator *(Color a, Color b) => new(a.r * b.r, a.g * b.g, a.b * b.b, a.a * b.a);
    public static Color operator *(Color a, Vector4 b) => new(a.r * b.x, a.g * b.y, a.b * b.z, a.a * b.w);
    public static Color operator *(Color a, float b) => new(a.r * b, a.g * b, a.b * b, a.a * b);
    public static Color operator *(float b, Color a) => new(a.r * b, a.g * b, a.b * b, a.a * b);
    public static Color operator /(Color a, float b) => new(a.r / b, a.g / b, a.b / b, a.a / b);
    public static bool operator ==(Color lhs, Color rhs) => (Vector4)lhs == (Vector4)rhs;
    public static bool operator !=(Color lhs, Color rhs) => !(lhs == rhs);

    public static Color Lerp(Color a, Color b, float t) => LerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Color Lerp(ref Color a, ref Color b, float t) => Lerp(a, b, t);

    public static Color LerpUnclamped(Color a, Color b, float t) =>
        new(a.r + (b.r - a.r) * t, a.g + (b.g - a.g) * t, a.b + (b.b - a.b) * t, a.a + (b.a - a.a) * t);

    public static Color LerpUnclamped(ref Color a, ref Color b, float t) => LerpUnclamped(a, b, t);

    /// <summary>The luminance, 0.299 r + 0.587 g + 0.114 b.</summary>
    public readonly float grayscale => 0.299f * r + 0.587f * g + 0.114f * b;

    /// <summary>The color converted from sRGB to linear space; alpha is unchanged.</summary>
    public readonly Color linear => new(Mathf.GammaToLinearSpace(r), Mathf.GammaToLinearSpace(g), Mathf.GammaToLinearSpace(b), a);

    /// <summary>The color converted from linear to sRGB space; alpha is unchanged.</summary>
    public readonly Color gamma => new(Mathf.LinearToGammaSpace(r), Mathf.LinearToGammaSpace(g), Mathf.LinearToGammaSpace(b), a);

    public readonly float maxColorComponent => Mathf.Max(Mathf.Max(r, g), b);

    public static implicit operator Vector4(Color c) => new(c.r, c.g, c.b, c.a);
    public static implicit operator Color(Vector4 v) => new(v.x, v.y, v.z, v.w);

    public float this[int index]
    {
        readonly get => index switch
        {
            0 => r,
            1 => g,
            2 => b,
            3 => a,
            _ => throw new IndexOutOfRangeException("Invalid Color index(" + index + ")!"),
        };
        set
        {
            switch (index)
            {
                case 0: r = value; break;
                case 1: g = value; break;
                case 2: b = value; break;
                case 3: a = value; break;
                default: throw new IndexOutOfRangeException("Invalid Color index(" + index + ")!");
            }
        }
    }

    /// <summary>Hue, saturation and value, each in [0, 1] for colors inside the RGB cube.</summary>
    public static void RGBToHSV(Color rgbColor, ref float H, ref float S, ref float V)
    {
        var max = rgbColor.maxColorComponent;
        var min = Mathf.Min(Mathf.Min(rgbColor.r, rgbColor.g), rgbColor.b);
        var delta = max - min;
        V = max;
        S = max > 0f ? delta / max : 0f;
        if (delta <= 0f)
        {
            H = 0f;
            return;
        }
        float sector;
        if (max == rgbColor.r) sector = (rgbColor.g - rgbColor.b) / delta;
        else if (max == rgbColor.g) sector = 2f + (rgbColor.b - rgbColor.r) / delta;
        else sector = 4f + (rgbColor.r - rgbColor.g) / delta;
        H = sector / 6f;
        if (H < 0f) H += 1f;
    }

    public static void RGBToHSV(ref Color rgbColor, ref float H, ref float S, ref float V) => RGBToHSV(rgbColor, ref H, ref S, ref V);

    public static Color HSVToRGB(float H, float S, float V) => HSVToRGB(H, S, V, true);

    /// <summary>Converts hue, saturation and value to RGB. Unless <paramref name="hdr"/> is true the
    /// result is clamped to [0, 1]; a value above 1 is otherwise kept.</summary>
    public static Color HSVToRGB(float H, float S, float V, bool hdr)
    {
        var color = new Color(V, V, V, 1f);
        if (S > 0f)
        {
            var sector = Mathf.Repeat(H, 1f) * 6f;
            var index = (int)MathF.Floor(sector);
            var fraction = sector - index;
            var p = V * (1f - S);
            var q = V * (1f - S * fraction);
            var t = V * (1f - S * (1f - fraction));
            color = index switch
            {
                0 => new Color(V, t, p, 1f),
                1 => new Color(q, V, p, 1f),
                2 => new Color(p, V, t, 1f),
                3 => new Color(p, q, V, 1f),
                4 => new Color(t, p, V, 1f),
                _ => new Color(V, p, q, 1f),
            };
        }
        if (hdr) return color;
        return new Color(Mathf.Clamp01(color.r), Mathf.Clamp01(color.g), Mathf.Clamp01(color.b), 1f);
    }

    public static Color aliceBlue => new(0.9411765f, 0.9725491f, 1f, 1f);
    public static Color antiqueWhite => new(0.9803922f, 0.9215687f, 0.8431373f, 1f);
    public static Color aquamarine => new(0.4980392f, 1f, 0.8313726f, 1f);
    public static Color azure => new(0.9411765f, 1f, 1f, 1f);
    public static Color beige => new(0.9607844f, 0.9607844f, 0.8627452f, 1f);
    public static Color bisque => new(1f, 0.8941177f, 0.7686275f, 1f);
    public static Color black => new(0f, 0f, 0f, 1f);
    public static Color blanchedAlmond => new(1f, 0.9215687f, 0.8039216f, 1f);
    public static Color blue => new(0f, 0f, 1f, 1f);
    public static Color blueViolet => new(0.5411765f, 0.1686275f, 0.8862746f, 1f);
    public static Color brown => new(0.6470588f, 0.1647059f, 0.1647059f, 1f);
    public static Color burlywood => new(0.8705883f, 0.7215686f, 0.5294118f, 1f);
    public static Color cadetBlue => new(0.372549f, 0.6196079f, 0.627451f, 1f);
    public static Color chartreuse => new(0.4980392f, 1f, 0f, 1f);
    public static Color chocolate => new(0.8235295f, 0.4117647f, 0.1176471f, 1f);
    public static Color clear => new(0f, 0f, 0f, 0f);
    public static Color coral => new(1f, 0.4980392f, 0.3137255f, 1f);
    public static Color cornflowerBlue => new(0.3921569f, 0.5843138f, 0.9294118f, 1f);
    public static Color cornsilk => new(1f, 0.9725491f, 0.8627452f, 1f);
    public static Color crimson => new(0.8627452f, 0.07843138f, 0.2352941f, 1f);
    public static Color cyan => new(0f, 1f, 1f, 1f);
    public static Color darkBlue => new(0f, 0f, 0.5450981f, 1f);
    public static Color darkCyan => new(0f, 0.5450981f, 0.5450981f, 1f);
    public static Color darkGoldenRod => new(0.7215686f, 0.5254902f, 0.04313726f, 1f);
    public static Color darkGray => new(0.6627451f, 0.6627451f, 0.6627451f, 1f);
    public static Color darkGreen => new(0f, 0.3921569f, 0f, 1f);
    public static Color darkKhaki => new(0.7411765f, 0.7176471f, 0.4196079f, 1f);
    public static Color darkMagenta => new(0.5450981f, 0f, 0.5450981f, 1f);
    public static Color darkOliveGreen => new(0.3333333f, 0.4196079f, 0.1843137f, 1f);
    public static Color darkOrange => new(1f, 0.5490196f, 0f, 1f);
    public static Color darkOrchid => new(0.6f, 0.1960784f, 0.8000001f, 1f);
    public static Color darkRed => new(0.5450981f, 0f, 0f, 1f);
    public static Color darkSalmon => new(0.9137256f, 0.5882353f, 0.4784314f, 1f);
    public static Color darkSeaGreen => new(0.5607843f, 0.7372549f, 0.5607843f, 1f);
    public static Color darkSlateBlue => new(0.282353f, 0.2392157f, 0.5450981f, 1f);
    public static Color darkSlateGray => new(0.1843137f, 0.3098039f, 0.3098039f, 1f);
    public static Color darkTurquoise => new(0f, 0.8078432f, 0.8196079f, 1f);
    public static Color darkViolet => new(0.5803922f, 0f, 0.8274511f, 1f);
    public static Color deepPink => new(1f, 0.07843138f, 0.5764706f, 1f);
    public static Color deepSkyBlue => new(0f, 0.7490196f, 1f, 1f);
    public static Color dimGray => new(0.4117647f, 0.4117647f, 0.4117647f, 1f);
    public static Color dodgerBlue => new(0.1176471f, 0.5647059f, 1f, 1f);
    public static Color firebrick => new(0.6980392f, 0.1333333f, 0.1333333f, 1f);
    public static Color floralWhite => new(1f, 0.9803922f, 0.9411765f, 1f);
    public static Color forestGreen => new(0.1333333f, 0.5450981f, 0.1333333f, 1f);
    public static Color gainsboro => new(0.8627452f, 0.8627452f, 0.8627452f, 1f);
    public static Color ghostWhite => new(0.9725491f, 0.9725491f, 1f, 1f);
    public static Color gold => new(1f, 0.8431373f, 0f, 1f);
    public static Color goldenRod => new(0.854902f, 0.6470588f, 0.1254902f, 1f);
    public static Color gray => new(0.5f, 0.5f, 0.5f, 1f);
    public static Color grey => new(0.5f, 0.5f, 0.5f, 1f);
    public static Color gray1 => new(0.1f, 0.1f, 0.1f, 1f);
    public static Color gray2 => new(0.2f, 0.2f, 0.2f, 1f);
    public static Color gray3 => new(0.3f, 0.3f, 0.3f, 1f);
    public static Color gray4 => new(0.4f, 0.4f, 0.4f, 1f);
    public static Color gray5 => new(0.5f, 0.5f, 0.5f, 1f);
    public static Color gray6 => new(0.6f, 0.6f, 0.6f, 1f);
    public static Color gray7 => new(0.7f, 0.7f, 0.7f, 1f);
    public static Color gray8 => new(0.8f, 0.8f, 0.8f, 1f);
    public static Color gray9 => new(0.9f, 0.9f, 0.9f, 1f);
    public static Color green => new(0f, 1f, 0f, 1f);
    public static Color greenYellow => new(0.6784314f, 1f, 0.1843137f, 1f);
    public static Color honeydew => new(0.9411765f, 1f, 0.9411765f, 1f);
    public static Color hotPink => new(1f, 0.4117647f, 0.7058824f, 1f);
    public static Color indianRed => new(0.8039216f, 0.3607843f, 0.3607843f, 1f);
    public static Color indigo => new(0.2941177f, 0f, 0.509804f, 1f);
    public static Color ivory => new(1f, 1f, 0.9411765f, 1f);
    public static Color khaki => new(0.9411765f, 0.9019608f, 0.5490196f, 1f);
    public static Color lavender => new(0.9019608f, 0.9019608f, 0.9803922f, 1f);
    public static Color lavenderBlush => new(1f, 0.9411765f, 0.9607844f, 1f);
    public static Color lawnGreen => new(0.4862745f, 0.9882354f, 0f, 1f);
    public static Color lemonChiffon => new(1f, 0.9803922f, 0.8039216f, 1f);
    public static Color lightBlue => new(0.6784314f, 0.8470589f, 0.9019608f, 1f);
    public static Color lightCoral => new(0.9411765f, 0.5019608f, 0.5019608f, 1f);
    public static Color lightCyan => new(0.8784314f, 1f, 1f, 1f);
    public static Color lightGoldenRod => new(0.9333334f, 0.8666667f, 0.509804f, 1f);
    public static Color lightGoldenRodYellow => new(0.9803922f, 0.9803922f, 0.8235295f, 1f);
    public static Color lightGray => new(0.8274511f, 0.8274511f, 0.8274511f, 1f);
    public static Color lightGreen => new(0.5647059f, 0.9333334f, 0.5647059f, 1f);
    public static Color lightPink => new(1f, 0.7137255f, 0.7568628f, 1f);
    public static Color lightSalmon => new(1f, 0.627451f, 0.4784314f, 1f);
    public static Color lightSeaGreen => new(0.1254902f, 0.6980392f, 0.6666667f, 1f);
    public static Color lightSkyBlue => new(0.5294118f, 0.8078432f, 0.9803922f, 1f);
    public static Color lightSlateBlue => new(0.5176471f, 0.4392157f, 1f, 1f);
    public static Color lightSlateGray => new(0.4666667f, 0.5333334f, 0.6f, 1f);
    public static Color lightSteelBlue => new(0.6901961f, 0.7686275f, 0.8705883f, 1f);
    public static Color lightYellow => new(1f, 1f, 0.8784314f, 1f);
    public static Color limeGreen => new(0.1960784f, 0.8039216f, 0.1960784f, 1f);
    public static Color linen => new(0.9803922f, 0.9411765f, 0.9019608f, 1f);
    public static Color magenta => new(1f, 0f, 1f, 1f);
    public static Color maroon => new(0.6901961f, 0.1882353f, 0.3764706f, 1f);
    public static Color mediumAquamarine => new(0.4f, 0.8039216f, 0.6666667f, 1f);
    public static Color mediumBlue => new(0f, 0f, 0.8039216f, 1f);
    public static Color mediumOrchid => new(0.7294118f, 0.3333333f, 0.8274511f, 1f);
    public static Color mediumPurple => new(0.5764706f, 0.4392157f, 0.8588236f, 1f);
    public static Color mediumSeaGreen => new(0.2352941f, 0.7019608f, 0.4431373f, 1f);
    public static Color mediumSlateBlue => new(0.482353f, 0.4078432f, 0.9333334f, 1f);
    public static Color mediumSpringGreen => new(0f, 0.9803922f, 0.6039216f, 1f);
    public static Color mediumTurquoise => new(0.282353f, 0.8196079f, 0.8000001f, 1f);
    public static Color mediumVioletRed => new(0.7803922f, 0.08235294f, 0.5215687f, 1f);
    public static Color midnightBlue => new(0.09803922f, 0.09803922f, 0.4392157f, 1f);
    public static Color mintCream => new(0.9607844f, 1f, 0.9803922f, 1f);
    public static Color mistyRose => new(1f, 0.8941177f, 0.882353f, 1f);
    public static Color moccasin => new(1f, 0.8941177f, 0.7098039f, 1f);
    public static Color navajoWhite => new(1f, 0.8705883f, 0.6784314f, 1f);
    public static Color navyBlue => new(0f, 0f, 0.5019608f, 1f);
    public static Color oldLace => new(0.9921569f, 0.9607844f, 0.9019608f, 1f);
    public static Color olive => new(0.5019608f, 0.5019608f, 0f, 1f);
    public static Color oliveDrab => new(0.4196079f, 0.5568628f, 0.1372549f, 1f);
    public static Color orange => new(1f, 0.6470588f, 0f, 1f);
    public static Color orangeRed => new(1f, 0.2705882f, 0f, 1f);
    public static Color orchid => new(0.854902f, 0.4392157f, 0.8392158f, 1f);
    public static Color paleGoldenRod => new(0.9333334f, 0.909804f, 0.6666667f, 1f);
    public static Color paleGreen => new(0.5960785f, 0.9843138f, 0.5960785f, 1f);
    public static Color paleTurquoise => new(0.6862745f, 0.9333334f, 0.9333334f, 1f);
    public static Color paleVioletRed => new(0.8588236f, 0.4392157f, 0.5764706f, 1f);
    public static Color papayaWhip => new(1f, 0.937255f, 0.8352942f, 1f);
    public static Color peachPuff => new(1f, 0.854902f, 0.7254902f, 1f);
    public static Color peru => new(0.8039216f, 0.5215687f, 0.2470588f, 1f);
    public static Color pink => new(1f, 0.7529413f, 0.7960785f, 1f);
    public static Color plum => new(0.8666667f, 0.627451f, 0.8666667f, 1f);
    public static Color powderBlue => new(0.6901961f, 0.8784314f, 0.9019608f, 1f);
    public static Color purple => new(0.627451f, 0.1254902f, 0.9411765f, 1f);
    public static Color rebeccaPurple => new(0.4f, 0.2f, 0.6f, 1f);
    public static Color red => new(1f, 0f, 0f, 1f);
    public static Color rosyBrown => new(0.7372549f, 0.5607843f, 0.5607843f, 1f);
    public static Color royalBlue => new(0.254902f, 0.4117647f, 0.882353f, 1f);
    public static Color saddleBrown => new(0.5450981f, 0.2705882f, 0.07450981f, 1f);
    public static Color salmon => new(0.9803922f, 0.5019608f, 0.4470589f, 1f);
    public static Color sandyBrown => new(0.9568628f, 0.6431373f, 0.3764706f, 1f);
    public static Color seaGreen => new(0.1803922f, 0.5450981f, 0.3411765f, 1f);
    public static Color seashell => new(1f, 0.9607844f, 0.9333334f, 1f);
    public static Color sienna => new(0.627451f, 0.3215686f, 0.1764706f, 1f);
    public static Color silver => new(0.7529413f, 0.7529413f, 0.7529413f, 1f);
    public static Color skyBlue => new(0.5294118f, 0.8078432f, 0.9215687f, 1f);
    public static Color slateBlue => new(0.4156863f, 0.3529412f, 0.8039216f, 1f);
    public static Color slateGray => new(0.4392157f, 0.5019608f, 0.5647059f, 1f);
    public static Color snow => new(1f, 0.9803922f, 0.9803922f, 1f);
    public static Color softRed => new(0.8627452f, 0.1921569f, 0.1960784f, 1f);
    public static Color softBlue => new(0.1882353f, 0.682353f, 0.7490196f, 1f);
    public static Color softGreen => new(0.5490196f, 0.7882354f, 0.1411765f, 1f);
    public static Color softYellow => new(1f, 0.9333334f, 0.5490196f, 1f);
    public static Color springGreen => new(0f, 1f, 0.4980392f, 1f);
    public static Color steelBlue => new(0.2745098f, 0.509804f, 0.7058824f, 1f);
    public static Color tan => new(0.8235295f, 0.7058824f, 0.5490196f, 1f);
    public static Color teal => new(0f, 0.5019608f, 0.5019608f, 1f);
    public static Color thistle => new(0.8470589f, 0.7490196f, 0.8470589f, 1f);
    public static Color tomato => new(1f, 0.3882353f, 0.2784314f, 1f);
    public static Color turquoise => new(0.2509804f, 0.8784314f, 0.8156863f, 1f);
    public static Color violet => new(0.9333334f, 0.509804f, 0.9333334f, 1f);
    public static Color violetRed => new(0.8156863f, 0.1254902f, 0.5647059f, 1f);
    public static Color wheat => new(0.9607844f, 0.8705883f, 0.7019608f, 1f);
    public static Color white => new(1f, 1f, 1f, 1f);
    public static Color whiteSmoke => new(0.9607844f, 0.9607844f, 0.9607844f, 1f);
    public static Color yellow => new(1f, 0.92156863f, 0.015686275f, 1f);
    public static Color yellowGreen => new(0.6039216f, 0.8039216f, 0.1960784f, 1f);
    public static Color yellowNice => new(1f, 0.92156863f, 0.015686275f, 1f);
}
