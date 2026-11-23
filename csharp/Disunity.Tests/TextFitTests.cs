using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// Best fit picks the largest size between resizeTextMinSize and resizeTextMaxSize at which the wrapped text fits the
/// extents, and alignByGeometry aligns the ink of each line instead of its advances. The synthetic font draws ink of 10 by
/// 14 at 20 pixels (0.7 of the size high) with an advance of 12 (0.6 of the size) and a line height of the size.
/// </summary>
internal static class TextFitTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var font = DisunityHost.RegisterFont(SyntheticFont.Bytes());
            BestFitIsTheLargestSizeThatFits(font);
            BestFitStaysInItsRange(font);
            BestFitWorksInScaledPixelsAndKeepsTheOverflowModes(font);
            PreferredSizesIgnoreBestFit(font);
            GeometryAlignmentUsesTheInk(font);
            GeometryAlignmentSkipsCharactersWithoutInk();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: TextGenerator picks best fit sizes and aligns lines by their ink");
    }

    private static TextGenerationSettings Fitting(Font font, int min, int max)
    {
        var settings = TextLayoutTests.Plain(font);
        (settings.resizeTextForBestFit, settings.resizeTextMinSize, settings.resizeTextMaxSize) = (true, min, max);
        return settings;
    }

    /// <summary>Whether the text at a plain size wraps to lines no wider than the extents and a block no taller than them.</summary>
    private static bool FitsAt(string text, TextGenerationSettings settings, int size)
    {
        settings.resizeTextForBestFit = false;
        settings.fontSize = size;
        settings.verticalOverflow = VerticalWrapMode.Overflow;
        settings.horizontalOverflow = HorizontalWrapMode.Wrap;
        var generator = TextLayoutTests.Lay(text, settings);
        var extents = settings.generationExtents * settings.scaleFactor;
        return generator.rectExtents.width <= extents.x + 1e-3f && generator.rectExtents.height <= extents.y + 1e-3f;
    }

    private static int LargestFit(string text, TextGenerationSettings settings, int min, int max)
    {
        for (var size = max; size > min; size--)
            if (FitsAt(text, settings, size)) return size;
        return min;
    }

    private static void BestFitIsTheLargestSizeThatFits(Font font)
    {
        foreach (var text in new[] { "AAAAAAAAAA", "AAA AAA AAA", "AB\nABABAB", "A" })
        {
            var settings = Fitting(font, 5, 60);
            var generator = TextLayoutTests.Lay(text, settings);
            var expected = LargestFit(text, Fitting(font, 5, 60), 5, 60);
            Expect.Equal(expected, generator.fontSizeUsedForBestFit, $"the best fit size of '{text}'");
            var ink = generator.verts[2].position.y - generator.verts[0].position.y;
            Expect.Near(-MathF.Ceiling(0.7f * expected), ink, $"'{text}' is drawn at that size", 0.51f);
        }
        var narrow = TextLayoutTests.Lay("AAAAAAAAAA", Fitting(font, 5, 60));
        Expect.That(narrow.fontSizeUsedForBestFit is > 5 and < 60, "the first text is neither at the minimum nor at the maximum");
    }

    private static void BestFitStaysInItsRange(Font font)
    {
        var small = Fitting(font, 8, 40);
        small.generationExtents = new Vector2(10f, 10f);
        Expect.Equal(8, TextLayoutTests.Lay("AAAA", small).fontSizeUsedForBestFit, "nothing fits, so the minimum");
        Expect.Equal(60, TextLayoutTests.Lay("A", Fitting(font, 5, 60)).fontSizeUsedForBestFit, "text that fits at the maximum uses it");
        Expect.Equal(30, TextLayoutTests.Lay("A", Fitting(font, 30, 10)).fontSizeUsedForBestFit, "a maximum below the minimum is the minimum");
        Expect.Equal(1, TextLayoutTests.Lay("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA", Fitting(font, 0, 1)).fontSizeUsedForBestFit, "a minimum of 0 is 1");
    }

    private static void BestFitWorksInScaledPixelsAndKeepsTheOverflowModes(Font font)
    {
        var scaled = Fitting(font, 5, 60);
        (scaled.scaleFactor, scaled.generationExtents) = (2f, new Vector2(60f, 50f));
        var generator = TextLayoutTests.Lay("AAA AAA AAA", scaled);
        var expected = LargestFit("AAA AAA AAA", scaled, 5, 60);
        Expect.Equal(expected, generator.fontSizeUsedForBestFit, "the size is before the scale factor and fits the scaled extents");
        var overflow = Fitting(font, 5, 60);
        overflow.horizontalOverflow = HorizontalWrapMode.Overflow;
        Expect.Equal(1, TextLayoutTests.Lay("AAA AAA AAA AAA", overflow).lineCount, "the final layout uses the settings' overflow, not the wrapping the fit used");
        Expect.Equal(LargestFit("AAA AAA AAA AAA", Fitting(font, 5, 60), 5, 60), TextLayoutTests.Lay("AAA AAA AAA AAA", overflow).fontSizeUsedForBestFit, "and the size is the one that fits wrapped");
    }

    private static void PreferredSizesIgnoreBestFit(Font font)
    {
        var generator = new TextGenerator();
        var settings = Fitting(font, 5, 60);
        Expect.Near(36f, generator.GetPreferredWidth("AAA", settings), "preferred width is at the settings' size");
        Expect.Near(20f, generator.GetPreferredHeight("AAA", settings), "preferred height is at the settings' size");
    }

    private static void GeometryAlignmentUsesTheInk(Font font)
    {
        var settings = TextLayoutTests.Plain(font);
        settings.textAnchor = TextAnchor.UpperCenter;
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("A", settings), 0, (54, 64, -16, -2), "by advances a single glyph starts at (120 - 12) / 2");
        settings.alignByGeometry = true;
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("A", settings), 0, (55, 65, -16, -2), "by ink it starts at (120 - 10) / 2");
        var ab = TextLayoutTests.Lay("AB", settings);
        TextLayoutTests.ExpectQuad(ab, 0, (49, 59, -16, -2), "two glyphs have 22 of ink");
        TextLayoutTests.ExpectQuad(ab, 1, (61, 71, -16, -2), "and the second follows the advance");
        settings.textAnchor = TextAnchor.UpperRight;
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("AB", settings), 1, (110, 120, -16, -2), "the last ink edge meets the right edge");
        settings.textAnchor = TextAnchor.UpperLeft;
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("AB", settings), 0, (0, 10, -16, -2), "the first ink edge meets the left edge");
    }

    private static void GeometryAlignmentSkipsCharactersWithoutInk()
    {
        var spaced = DisunityHost.RegisterFont(SyntheticFont.Bytes(emptySpace: true));
        var settings = TextLayoutTests.Plain(spaced);
        settings.alignByGeometry = true;
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay(" A", settings), 0, (0, 10, -16, -2), "a leading space does not move the ink off the left edge");
        settings.textAnchor = TextAnchor.UpperRight;
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("A ", settings), 0, (110, 120, -16, -2), "a trailing space does not move the ink off the right edge");
        settings.alignByGeometry = false;
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("A ", settings), 0, (102, 112, -16, -2), "without geometry alignment the trailing space counts");
    }
}
