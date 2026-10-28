using System.Globalization;

namespace UnityEngine;

/// <summary>An RGBA color of four bytes. Converting from <see cref="Color"/> multiplies each
/// channel by 255, rounds to the nearest integer and clamps to 0..255.</summary>
public struct Color32 : IEquatable<Color32>, IFormattable
{
    public byte r;
    public byte g;
    public byte b;
    public byte a;

    public Color32(byte r, byte g, byte b, byte a)
    {
        this.r = r;
        this.g = g;
        this.b = b;
        this.a = a;
    }

    public static implicit operator Color32(Color c) => new(ToByte(c.r), ToByte(c.g), ToByte(c.b), ToByte(c.a));
    public static implicit operator Color(Color32 c) => new(c.r / 255f, c.g / 255f, c.b / 255f, c.a / 255f);

    private static byte ToByte(float channel) => (byte)Mathf.Clamp(Mathf.RoundToInt(channel * 255f), 0, 255);

    public static Color32 Lerp(Color32 a, Color32 b, float t) => LerpUnclamped(a, b, Mathf.Clamp01(t));
    public static Color32 Lerp(ref Color32 a, ref Color32 b, float t) => Lerp(a, b, t);

    /// <summary>Each channel is truncated toward zero after interpolating, and kept within 0..255.</summary>
    public static Color32 LerpUnclamped(Color32 a, Color32 b, float t) => new(
        Channel(a.r, b.r, t), Channel(a.g, b.g, t), Channel(a.b, b.b, t), Channel(a.a, b.a, t));

    public static Color32 LerpUnclamped(ref Color32 a, ref Color32 b, float t) => LerpUnclamped(a, b, t);

    private static byte Channel(byte from, byte to, float t) => (byte)Mathf.Clamp((int)(from + (to - from) * t), 0, 255);

    public byte this[int index]
    {
        readonly get => index switch
        {
            0 => r,
            1 => g,
            2 => b,
            3 => a,
            _ => throw new IndexOutOfRangeException("Invalid Color32 index(" + index + ")!"),
        };
        set
        {
            switch (index)
            {
                case 0: r = value; break;
                case 1: g = value; break;
                case 2: b = value; break;
                case 3: a = value; break;
                default: throw new IndexOutOfRangeException("Invalid Color32 index(" + index + ")!");
            }
        }
    }

    public override readonly int GetHashCode() => r | (g << 8) | (b << 16) | (a << 24);
    public override readonly bool Equals(object? other) => other is Color32 color && Equals(color);
    public readonly bool Equals(Color32 other) => r == other.r && g == other.g && b == other.b && a == other.a;
    public readonly bool Equals(ref Color32 other) => Equals(other);

    public override readonly string ToString() => ToString(null, null);
    public readonly string ToString(string? format) => ToString(format, null);

    public readonly string ToString(string? format, IFormatProvider? formatProvider)
    {
        formatProvider ??= CultureInfo.InvariantCulture.NumberFormat;
        return $"RGBA({r.ToString(format, formatProvider)}, {g.ToString(format, formatProvider)}, {b.ToString(format, formatProvider)}, {a.ToString(format, formatProvider)})";
    }
}
