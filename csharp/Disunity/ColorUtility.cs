namespace UnityEngine;

/// <summary>Converts colors to and from HTML color text.</summary>
public static class ColorUtility
{
    /// <summary>The named colors as 0xRRGGBB, with their standard HTML values.</summary>
    private static readonly Dictionary<string, uint> Names = new(StringComparer.OrdinalIgnoreCase)
    {
        ["red"] = 0xFF0000, ["cyan"] = 0x00FFFF, ["blue"] = 0x0000FF, ["darkblue"] = 0x00008B,
        ["lightblue"] = 0xADD8E6, ["purple"] = 0x800080, ["yellow"] = 0xFFFF00, ["lime"] = 0x00FF00,
        ["fuchsia"] = 0xFF00FF, ["white"] = 0xFFFFFF, ["silver"] = 0xC0C0C0, ["grey"] = 0x808080,
        ["gray"] = 0x808080, ["black"] = 0x000000, ["orange"] = 0xFFA500, ["brown"] = 0xA52A2A,
        ["maroon"] = 0x800000, ["green"] = 0x008000, ["olive"] = 0x808000, ["navy"] = 0x000080,
        ["teal"] = 0x008080, ["aqua"] = 0x00FFFF, ["magenta"] = 0xFF00FF,
    };

    /// <summary>
    /// Reads "#RGB", "#RGBA", "#RRGGBB" or "#RRGGBBAA" (hex digits in either case; a short form doubles
    /// each digit) or one of the color names, in any case. Alpha is 1 unless the text gives it. On failure
    /// the color is the default color and the result is false.
    /// </summary>
    public static bool TryParseHtmlString(string htmlString, out Color color)
    {
        color = default;
        if (string.IsNullOrEmpty(htmlString)) return false;
        if (htmlString[0] == '#') return TryParseHex(htmlString.AsSpan(1), out color);
        if (!Names.TryGetValue(htmlString, out var rgb)) return false;
        color = FromBytes((byte)(rgb >> 16), (byte)(rgb >> 8), (byte)rgb, 255);
        return true;
    }

    /// <summary>Six uppercase hex digits, without '#'. Channels clamp to 0 to 1; a half rounds up.</summary>
    public static string ToHtmlStringRGB(Color color) => $"{Channel(color.r):X2}{Channel(color.g):X2}{Channel(color.b):X2}";

    /// <summary>Eight uppercase hex digits, without '#'.</summary>
    public static string ToHtmlStringRGBA(Color color) => ToHtmlStringRGB(color) + $"{Channel(color.a):X2}";

    private static byte Channel(float value) =>
        value > 0 ? (byte)MathF.Round(MathF.Min(value, 1f) * 255f, MidpointRounding.AwayFromZero) : (byte)0;

    private static Color FromBytes(byte r, byte g, byte b, byte a) => new(r / 255f, g / 255f, b / 255f, a / 255f);

    private static bool TryParseHex(ReadOnlySpan<char> digits, out Color color)
    {
        color = default;
        var shortForm = digits.Length is 3 or 4;
        if (!shortForm && digits.Length is not (6 or 8)) return false;
        Span<byte> bytes = stackalloc byte[4];
        bytes.Fill(255);
        var step = shortForm ? 1 : 2;
        for (var i = 0; i < digits.Length / step; i++)
        {
            var high = digits[i * step];
            var low = shortForm ? high : digits[i * step + 1];
            if (!Uri.IsHexDigit(high) || !Uri.IsHexDigit(low)) return false;
            bytes[i] = (byte)(Uri.FromHex(high) * 16 + Uri.FromHex(low));
        }
        color = FromBytes(bytes[0], bytes[1], bytes[2], bytes[3]);
        return true;
    }
}
