using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// TextGenerator applies the pair adjustments of the font's kern table to the advance between two glyphs of one size on
/// one line. The synthetic font gives every printable character glyph 1, and its kern table here lowers the pair (1, 1) by
/// 100 units, which is 2 pixels at 20 pixels per em.
/// </summary>
internal static class TextKerningTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var kerned = DisunityHost.RegisterFont(SyntheticFont.Bytes(kern: [(1, 1, -100)]));
            var plain = DisunityHost.RegisterFont(SyntheticFont.Bytes());
            AKernedPairMovesTheSecondGlyph(kerned);
            AFontWithoutKerningIsUnchanged(plain);
            KerningCountsInWidthsAndWrapping(kerned);
            KerningRestartsAtEveryLineStart(kerned);
            KerningPairsGlyphsOfOneSizeOnly(kerned);
            GeometryAlignmentIncludesKerning(kerned);
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: TextGenerator applies kern table pairs between glyphs of one size on one line");
    }

    private static void AKernedPairMovesTheSecondGlyph(Font font)
    {
        var generator = TextLayoutTests.Lay("AAA", TextLayoutTests.Plain(font));
        TextLayoutTests.ExpectQuad(generator, 0, (0, 10, -16, -2), "the first glyph is where it was");
        TextLayoutTests.ExpectQuad(generator, 1, (10, 20, -16, -2), "the second moves left by 2");
        TextLayoutTests.ExpectQuad(generator, 2, (20, 30, -16, -2), "and the third by 4");
        Expect.That(generator.characters[1].cursorPos.x == 10f && generator.characters[1].charWidth == 12f, "the cursor includes the kerning and the width does not");
        var rich = Rich(font);
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("<color=#ff0000>A</color>A", rich), 1, (10, 20, -16, -2), "tags between the glyphs do not stop the pair");
    }

    private static TextGenerationSettings Rich(Font font)
    {
        var settings = TextLayoutTests.Plain(font);
        settings.richText = true;
        return settings;
    }

    private static void AFontWithoutKerningIsUnchanged(Font font) =>
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("AA", TextLayoutTests.Plain(font)), 1, (12, 22, -16, -2), "no kern table, no adjustment");

    private static void KerningCountsInWidthsAndWrapping(Font font)
    {
        var generator = new TextGenerator();
        Expect.Near(32f, generator.GetPreferredWidth("AAA", TextLayoutTests.Plain(font)), "preferred width is 36 less two pairs of 2");
        var narrow = TextLayoutTests.Plain(font);
        narrow.generationExtents = new Vector2(55f, 100f);
        var wrapped = TextLayoutTests.Lay("AAAAAAAA", narrow);
        Expect.That(wrapped.lineCount == 2 && wrapped.lines[1].startCharIdx == 5, "five kerned glyphs fit in 55 where four unkerned ones do");
    }

    private static void KerningRestartsAtEveryLineStart(Font font)
    {
        var narrow = TextLayoutTests.Plain(font);
        narrow.generationExtents = new Vector2(55f, 100f);
        var wrapped = TextLayoutTests.Lay("AAAAAAAA", narrow);
        TextLayoutTests.ExpectQuad(wrapped, 5, (0, 10, -36, -22), "a wrapped line starts at the left edge, with no pair against the line before");
        TextLayoutTests.ExpectQuad(wrapped, 6, (10, 20, -36, -22), "and its second glyph is kerned against its first");
        var lines = TextLayoutTests.Lay("A\nAA", TextLayoutTests.Plain(font));
        TextLayoutTests.ExpectQuad(lines, 1, (0, 10, -36, -22), "a newline restarts the pair state");
        TextLayoutTests.ExpectQuad(lines, 2, (10, 20, -36, -22), "and the next pair is kerned");
    }

    private static void KerningPairsGlyphsOfOneSizeOnly(Font font)
    {
        var mixed = TextLayoutTests.Lay("A<size=40>A</size>", Rich(font));
        TextLayoutTests.ExpectQuad(mixed, 1, (12, 32, -32, -4), "glyphs of different sizes are not kerned");
        var big = TextLayoutTests.Lay("<size=40>AA</size>", Rich(font));
        TextLayoutTests.ExpectQuad(big, 1, (20, 40, -32, -4), "a pair at 40 pixels is adjusted by 4");
    }

    private static void GeometryAlignmentIncludesKerning(Font font)
    {
        var settings = TextLayoutTests.Plain(font);
        (settings.alignByGeometry, settings.textAnchor) = (true, TextAnchor.UpperRight);
        var generator = TextLayoutTests.Lay("AA", settings);
        TextLayoutTests.ExpectQuad(generator, 0, (100, 110, -16, -2), "the ink of two kerned glyphs is 20 wide");
        TextLayoutTests.ExpectQuad(generator, 1, (110, 120, -16, -2), "and ends at the right edge");
    }
}
