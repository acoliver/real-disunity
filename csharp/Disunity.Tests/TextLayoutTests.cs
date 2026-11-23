using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// TextGenerator lays plain text out into UIVertex quads, UICharInfo and UILineInfo values the way UI Text consumes them.
/// The synthetic font at 20 pixels has ascent 16, line height 20, and for every code point 32 to 126 a triangle glyph with
/// ink 10 by 14, bearing (0, 14) and advance 12. Unless a test says otherwise the text rectangle is 120 by 100 with the
/// pivot at its top left corner, so the origin is that corner and y grows upward.
/// </summary>
internal static class TextLayoutTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var font = DisunityHost.RegisterFont(SyntheticFont.Bytes());
            EachInkedGlyphIsAQuad(font);
            LeftAlignedGlyphsSitOnTheBaseline(font);
            AnchorsPlaceTheBlock(font);
            WrappingBreaksAtSpacesAndLongWords(font);
            TruncationDropsLinesThatPassTheBottom(font);
            LineSpacingMovesTheNextBaseline(font);
            ScaleFactorAndPivotMoveTheResult(font);
            UvsPointAtTheAtlas(font);
            CharactersLinesAndExtentsDescribeTheResult(font);
            PreferredSizesLeaveTheResultAlone(font);
            AFontThatCannotLayOutFails(font);
            ASecondPopulateOfTheSameTextIsCached(font);
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: TextGenerator lays plain text out as quads, characters, lines and extents");
    }

    internal static TextGenerationSettings Plain(Font? font) => new()
    {
        font = font!, color = Color.white, fontSize = 20, lineSpacing = 1f, scaleFactor = 1f, textAnchor = TextAnchor.UpperLeft,
        generationExtents = new Vector2(120f, 100f), pivot = new Vector2(0f, 1f),
    };

    internal static TextGenerator Lay(string text, TextGenerationSettings settings)
    {
        var generator = new TextGenerator();
        Expect.That(generator.Populate(text, settings), $"Populate of '{text}'");
        return generator;
    }

    /// <summary>The bounds of quad <paramref name="index"/>: left, right, bottom, top.</summary>
    internal static (float L, float R, float B, float T) Quad(TextGenerator generator, int index)
    {
        var v = generator.verts;
        var quad = new[] { v[index * 4], v[index * 4 + 1], v[index * 4 + 2], v[index * 4 + 3] }.Select(q => q.position).ToArray();
        return (quad.Min(p => p.x), quad.Max(p => p.x), quad.Min(p => p.y), quad.Max(p => p.y));
    }

    internal static void ExpectQuad(TextGenerator generator, int index, (float L, float R, float B, float T) expected, string what)
    {
        var quad = Quad(generator, index);
        Expect.That(Math.Abs(quad.L - expected.L) + Math.Abs(quad.R - expected.R) + Math.Abs(quad.B - expected.B) + Math.Abs(quad.T - expected.T) < 1e-3f,
            $"{what}: expected {expected}, got {quad}");
    }

    private static void EachInkedGlyphIsAQuad(Font font)
    {
        var hello = Lay("Hello", Plain(font));
        Expect.That(hello.vertexCount == 20 && hello.verts.Count == 20 && hello.GetVerticesArray().Length == 20, "five glyphs make 20 vertices");
        Expect.That(hello.characterCount == 5 && hello.lineCount == 1 && hello.characterCountVisible == 5, "counts of one line");
        var spaced = DisunityHost.RegisterFont(SyntheticFont.Bytes(emptySpace: true));
        var gap = Lay("a b", Plain(spaced));
        Expect.That(gap.vertexCount == 8 && gap.characterCount == 3, "a space with no outline has a character and no quad");
        Expect.Near(6f, gap.characters[1].charWidth, "and advances by its own width");
        Expect.Near(18f, gap.characters[2].cursorPos.x, "so the next glyph starts after it");
    }

    private static void LeftAlignedGlyphsSitOnTheBaseline(Font font)
    {
        var ab = Lay("AB", Plain(font));
        ExpectQuad(ab, 0, (0, 10, -16, -2), "first glyph");
        ExpectQuad(ab, 1, (12, 22, -16, -2), "second glyph");
        var first = ab.verts.Take(4).Select(v => (v.position.x, v.position.y)).ToArray();
        Expect.Sequence(new[] { (0f, -2f), (10f, -2f), (10f, -16f), (0f, -16f) }, first, "vertices run top left, top right, bottom right, bottom left");
        Expect.That(ab.verts.All(v => v.color.Equals(new Color32(255, 255, 255, 255)) && v.normal == new Vector3(0, 0, -1)), "vertices are white and face the camera");
    }

    private static void AnchorsPlaceTheBlock(Font font)
    {
        var anchors = new (TextAnchor Anchor, float X, float Baseline)[]
        {
            (TextAnchor.UpperCenter, 48, -16), (TextAnchor.UpperRight, 96, -16), (TextAnchor.MiddleLeft, 0, -56),
            (TextAnchor.MiddleCenter, 48, -56), (TextAnchor.LowerRight, 96, -96), (TextAnchor.LowerLeft, 0, -96),
        };
        foreach (var (anchor, x, baseline) in anchors)
        {
            var settings = Plain(font);
            settings.textAnchor = anchor;
            ExpectQuad(Lay("AB", settings), 0, (x, x + 10, baseline, baseline + 14), anchor.ToString());
        }
    }

    private static void WrappingBreaksAtSpacesAndLongWords(Font font)
    {
        var narrow = Plain(font);
        narrow.generationExtents = new Vector2(60f, 100f);
        var words = Lay("AAA AAA", narrow);
        Expect.That(words.lineCount == 2 && words.lines[1].startCharIdx == 4, "the second line starts after the consumed space");
        Expect.That(words.vertexCount == 24 && words.characterCount == 7, "six quads, and the space has a character");
        ExpectQuad(words, 3, (0, 10, -36, -22), "the second line's first glyph");
        narrow.horizontalOverflow = HorizontalWrapMode.Overflow;
        var overflow = Lay("AAA AAA", narrow);
        Expect.That(overflow.lineCount == 1 && Math.Abs(overflow.rectExtents.width - 84f) < 1e-3f, "overflow keeps one line of the full width");
        narrow.horizontalOverflow = HorizontalWrapMode.Wrap;
        var word = Lay("AAAAAAAA", narrow);
        Expect.That(word.lineCount == 2 && word.lines[1].startCharIdx == 5, "an overlong word breaks before the character that does not fit");
        Expect.Equal(2, Lay("A\nB", Plain(font)).lineCount, "a newline ends a line");
        var crlf = Lay("A\r\nB", Plain(font));
        Expect.That(crlf.lineCount == 2 && crlf.characterCount == 3, "a carriage return is skipped");
        Expect.Equal(2, Lay("A\n", Plain(font)).lineCount, "a trailing newline starts an empty line");
    }

    private static void TruncationDropsLinesThatPassTheBottom(Font font)
    {
        var settings = Plain(font);
        settings.generationExtents = new Vector2(60f, 30f);
        var truncated = Lay("AAA AAA", settings);
        Expect.That(truncated.lineCount == 1 && truncated.characterCountVisible == 4 && truncated.characterCount == 7, "the second line is dropped");
        Expect.Equal(12, truncated.vertexCount, "only the first line has quads");
        settings.generateOutOfBounds = true;
        Expect.Equal(2, Lay("AAA AAA", settings).lineCount, "generateOutOfBounds keeps both lines");
        settings.generateOutOfBounds = false;
        settings.verticalOverflow = VerticalWrapMode.Overflow;
        Expect.Equal(2, Lay("AAA AAA", settings).lineCount, "vertical overflow keeps both lines");
    }

    private static void LineSpacingMovesTheNextBaseline(Font font)
    {
        var settings = Plain(font);
        settings.lineSpacing = 1.5f;
        var spaced = Lay("A\nB", settings);
        ExpectQuad(spaced, 1, (0, 10, -46, -32), "the second line is 30 below the first");
        Expect.Near(10f, spaced.lines[0].leading, "leading is the step minus the line height");
    }

    private static void ScaleFactorAndPivotMoveTheResult(Font font)
    {
        var settings = Plain(font);
        settings.fontSize = 10;
        settings.scaleFactor = 2f;
        settings.generationExtents = new Vector2(30f, 25f);
        var scaled = Lay("AAA AAA", settings);
        Expect.That(scaled.lineCount == 2 && scaled.lines[1].startCharIdx == 4, "scale factor wraps as the doubled size does");
        ExpectQuad(scaled, 3, (0, 10, -36, -22), "pixels are the scaled size");
        var centred = Plain(font);
        centred.pivot = new Vector2(0.5f, 0.5f);
        ExpectQuad(Lay("AB", centred), 0, (-60, -50, 50 - 16, 50 - 2), "the pivot moves the text rectangle");
    }

    private static void UvsPointAtTheAtlas(Font font)
    {
        var generator = Lay("AAB", Plain(font));
        var atlas = (Texture2D)font.material.mainTexture!;
        Expect.That(atlas.format == TextureFormat.Alpha8 && atlas.width > 1, "the material's texture is the atlas");
        Expect.That(generator.verts.All(v => v.uv0.x is >= 0 and <= 1 && v.uv0.y is >= 0 and <= 1), "UVs are in the atlas");
        var tl = generator.verts[0].uv0;
        var br = generator.verts[2].uv0;
        Expect.Near(10f / atlas.width, br.x - tl.x, "the quad's UV width is the glyph's");
        Expect.Near(14f / atlas.height, tl.y - br.y, "the quad's UV height is the glyph's; v grows upward");
        var raw = atlas.GetRawTextureData();
        var (x, y) = ((int)MathF.Round(generator.verts[3].uv0.x * atlas.width), (int)MathF.Round(generator.verts[3].uv0.y * atlas.height));
        Expect.That(raw[y * atlas.width + x] > 200, "the bottom-left corner of the quad is inked");
        Expect.That(raw[(y + 13) * atlas.width + x + 9] == 0, "the top-right corner of the quad is empty");
        Expect.That(generator.verts[0].uv0 == generator.verts[4].uv0, "the same character has the same UVs");
        var before = raw.ToArray();
        Lay("AAB", Plain(font));
        Expect.Sequence(before, atlas.GetRawTextureData(), "laying out again leaves the atlas as it was");
    }

    private static void CharactersLinesAndExtentsDescribeTheResult(Font font)
    {
        var settings = Plain(font);
        settings.lineSpacing = 1.5f;
        var generator = Lay("AB\nC", settings);
        Expect.That(generator.characterCount == 4 && generator.characters.Count == 4 && generator.GetCharactersArray().Length == 4, "a character per laid out character");
        Expect.That(generator.characters[1].cursorPos == new Vector2(12f, 0f) && Math.Abs(generator.characters[1].charWidth - 12f) < 1e-3f, "cursor at the pen and the line's top");
        Expect.That(generator.characters[3].cursorPos == new Vector2(0f, -30f), "the third line starts a step lower");
        var lines = generator.GetLinesArray();
        Expect.That(lines.Length == 2 && lines[1].startCharIdx == 3 && lines[1].height == 20 && Math.Abs(lines[1].topY + 30f) < 1e-3f, "line info");
        Expect.Equal(new Rect(0f, -50f, 24f, 50f), generator.rectExtents, "the extents cover both lines");
        Expect.Equal(20, generator.fontSizeUsedForBestFit, "the size used");
    }

    private static void PreferredSizesLeaveTheResultAlone(Font font)
    {
        var generator = Lay("AB", Plain(font));
        var narrow = Plain(font);
        narrow.generationExtents = new Vector2(60f, 100f);
        Expect.Near(36f, generator.GetPreferredWidth("AAA", Plain(font)), "preferred width of AAA");
        Expect.Near(20f, generator.GetPreferredHeight("AAA", Plain(font)), "preferred height of AAA");
        Expect.Near(84f, generator.GetPreferredWidth("AAA AAA", narrow), "preferred width ignores wrapping");
        Expect.Near(40f, generator.GetPreferredHeight("AAA AAA", narrow), "preferred height wraps at the extents");
        Expect.Near(0f, generator.GetPreferredWidth("", Plain(font)), "preferred width of nothing");
        Expect.That(generator.vertexCount == 8 && generator.characterCount == 2, "the generator's result is untouched");
    }

    private static void AFontThatCannotLayOutFails(Font font)
    {
        var generator = new TextGenerator();
        Expect.That(!generator.Populate("A", Plain(null)), "no font");
        Expect.That(!generator.Populate("A", Plain(new Font("Empty", []))), "a font with no file");
        var logged = new List<string>();
        Application.LogCallback collect = (message, _, type) => { if (type == LogType.Error) logged.Add(message); };
        Application.logMessageReceived += collect;
        try
        {
            var empty = new Font("Empty", []);
            Expect.That(!generator.PopulateWithErrors("A", Plain(empty), null!) && !generator.PopulateWithErrors("B", Plain(empty), null!), "PopulateWithErrors fails");
            Expect.Equal(1, logged.Count(m => m.Contains("Empty")), "the reason is logged once per font");
        }
        finally { Application.logMessageReceived -= collect; }
        Expect.That(generator.vertexCount == 0, "a failed populate leaves an empty result");
        DisunityHost.UnloadWorld();
        Expect.That(!generator.Populate("A", Plain(font)), "a font whose world is gone");
        DisunityHost.LoadWorld();
        Expect.That(generator.Populate("A", Plain(font)) && generator.vertexCount == 4, "and the same font in the next world");
    }

    private static void ASecondPopulateOfTheSameTextIsCached(Font font)
    {
        var generator = new TextGenerator();
        var settings = Plain(font);
        generator.Populate("AB", settings);
        var first = generator.GetVerticesArray();
        Expect.That(generator.Populate("AB", settings), "an unchanged populate succeeds");
        Expect.Sequence(first.Select(v => v.position), generator.GetVerticesArray().Select(v => v.position), "with the same vertices");
        settings.textAnchor = TextAnchor.LowerRight;
        generator.Populate("AB", settings);
        Expect.That(generator.verts[0].position != first[0].position, "a changed setting lays out again");
        generator.Invalidate();
        Expect.That(generator.vertexCount == 0 && generator.characterCount == 0 && generator.lineCount == 0, "Invalidate clears the result");
        Expect.That(generator.Populate("AB", settings) && generator.vertexCount == 8, "and the next populate fills it again");
    }
}
