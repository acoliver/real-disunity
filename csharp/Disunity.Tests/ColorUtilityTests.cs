using UnityEngine;

/// <summary>ColorUtility parses HTML color text and writes it back.</summary>
internal static class ColorUtilityTests
{
    public static void Run()
    {
        ParsesHex();
        ParsesNames();
        RejectsBadText();
        WritesHex();
        Console.WriteLine("PASS: ColorUtility parses HTML hex and names and writes hex strings");
    }

    private static Color Parse(string text)
    {
        Expect.That(ColorUtility.TryParseHtmlString(text, out var color), $"{text} parses");
        return color;
    }

    private static void Same(Color expected, Color actual, string what)
    {
        Expect.Near(expected.r, actual.r, what + " r");
        Expect.Near(expected.g, actual.g, what + " g");
        Expect.Near(expected.b, actual.b, what + " b");
        Expect.Near(expected.a, actual.a, what + " a");
    }

    private static void ParsesHex()
    {
        Same(new Color(1, 0, 0, 1), Parse("#F00"), "#F00");
        Same(new Color(1, 0, 0, 0.5333333f), Parse("#f008"), "#f008");
        Same(new Color(0x12 / 255f, 0x34 / 255f, 0x56 / 255f, 1), Parse("#123456"), "#123456");
        Same(new Color(0xAB / 255f, 0xCD / 255f, 0xEF / 255f, 0x01 / 255f), Parse("#abcdef01"), "#abcdef01");
        Same(new Color(0xAA / 255f, 0xBB / 255f, 0xCC / 255f, 1), Parse("#ABC"), "short form expands each digit");
    }

    private static void ParsesNames()
    {
        Same(new Color(0xAD / 255f, 0xD8 / 255f, 0xE6 / 255f, 1), Parse("lightblue"), "lightblue");
        Same(new Color(1, 0xA5 / 255f, 0, 1), Parse("Orange"), "Orange is case-insensitive");
        Same(new Color(0xA5 / 255f, 0x2A / 255f, 0x2A / 255f, 1), Parse("BROWN"), "BROWN");
        Same(new Color(0, 0, 0x8B / 255f, 1), Parse("darkblue"), "darkblue");
        Same(new Color(0.5019608f, 0.5019608f, 0.5019608f, 1), Parse("grey"), "grey");
        Same(Parse("grey"), Parse("gray"), "gray is grey");
        Same(new Color(0, 0x80 / 255f, 0, 1), Parse("green"), "green");
        Same(new Color(0, 1, 0, 1), Parse("lime"), "lime");
        Same(new Color(0xC0 / 255f, 0xC0 / 255f, 0xC0 / 255f, 1), Parse("silver"), "silver");
        foreach (var name in new[] { "red", "cyan", "blue", "purple", "yellow", "fuchsia", "white", "black", "maroon", "olive", "navy", "teal", "aqua", "magenta" })
            Expect.That(ColorUtility.TryParseHtmlString(name, out _), name + " is a known name");
        Same(Parse("cyan"), Parse("aqua"), "aqua is cyan");
        Same(Parse("magenta"), Parse("fuchsia"), "fuchsia is magenta");
        Same(new Color(0x80 / 255f, 0, 0, 1), Parse("maroon"), "maroon");
        Same(new Color(0x80 / 255f, 0, 0x80 / 255f, 1), Parse("purple"), "purple");
    }

    private static void RejectsBadText()
    {
        foreach (var text in new[] { "", "#", "#12", "#12345", "#1234567", "#123456789", "#GGG", "123456", "#12 456", "nocolor", "#red", "light blue" })
        {
            var parsed = ColorUtility.TryParseHtmlString(text, out var color);
            Expect.That(!parsed, $"'{text}' is refused");
            Expect.That(color == default, $"'{text}' leaves the default color");
        }
        Expect.That(!ColorUtility.TryParseHtmlString(null!, out var none) && none == default, "null is refused");
        Expect.That(ColorUtility.TryParseHtmlString("red", out _) && !ColorUtility.TryParseHtmlString("#zzz", out var after) && after == default, "a refusal after a success still gives the default");
    }

    private static void WritesHex()
    {
        Expect.Equal("FF8000", ColorUtility.ToHtmlStringRGB(new Color(1f, 0.5f, 0f)), "RGB");
        Expect.Equal("FF800040", ColorUtility.ToHtmlStringRGBA(new Color(1f, 0.5f, 0f, 0.25f)), "RGBA");
        Expect.Equal("00FF00", ColorUtility.ToHtmlStringRGB(new Color(-3f, 7f, 0f)), "channels clamp to 0..1");
        Expect.Equal("000000FF", ColorUtility.ToHtmlStringRGBA(new Color(0, 0, 0, 2)), "alpha clamps");
        Expect.Equal("0A141E", ColorUtility.ToHtmlStringRGB(new Color(10 / 255f, 20 / 255f, 30 / 255f)), "round trip of bytes");
        Expect.Equal("FFFFFF", ColorUtility.ToHtmlStringRGB(Parse("#ffffff")), "parse then write");
    }
}
