using Disunity.Hosting;
using UnityEngine;

/// <summary>
/// With rich text on, TextGenerator reads the tags b, i, size and color, lays out the text between them with the style
/// they give, and draws every other tag, and every tag that is invalid or never closed, as the characters it is written
/// with. The synthetic font draws ink of 10 by 14 at 20 pixels, so a size of 40 doubles it to 20 by 28.
/// </summary>
internal static class TextRichTests
{
    private static readonly Color32 White = new(255, 255, 255, 255);

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var font = DisunityHost.RegisterFont(SyntheticFont.Bytes());
            ColorTagsColorTheirCharacters(font);
            BoldAndItalicDrawTheRegularFace(font);
            SizeTagsSetTheSizeOfTheirCharacters(font);
            MixedSizesShareABaselineAndTakeTheLargestLineHeight(font);
            TagsThatAreNotUnderstoodDrawAsWritten(font);
            WithRichTextOffEveryTagIsText(font);
            PreferredSizesSkipTheTags(font);
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: TextGenerator lays rich text out with b, i, size and color, and draws other tags as written");
    }

    private static TextGenerationSettings Rich(Font font)
    {
        var settings = TextLayoutTests.Plain(font);
        settings.richText = true;
        return settings;
    }

    private static void ColorTagsColorTheirCharacters(Font font)
    {
        var two = TextLayoutTests.Lay("<color=#ff0000>A</color>B", Rich(font));
        Expect.That(two.vertexCount == 8 && two.characterCount == 2, "the tags are not characters");
        Expect.That(two.verts[0].color.Equals(new Color32(255, 0, 0, 255)) && two.verts[3].color.Equals(new Color32(255, 0, 0, 255)), "the tagged glyph is red");
        Expect.That(two.verts[4].color.Equals(White), "the glyph after the tag is white again");
        Expect.That(TextLayoutTests.Lay("<color=#f00>A</color>", Rich(font)).verts[0].color.Equals(new Color32(255, 0, 0, 255)), "a short hex colour");
        Expect.That(TextLayoutTests.Lay("<color=red>A</color>", Rich(font)).verts[0].color.Equals(new Color32(255, 0, 0, 255)), "a named colour");
        Expect.That(TextLayoutTests.Lay("<color=#ff000080>A</color>", Rich(font)).verts[0].color.Equals(new Color32(255, 0, 0, 128)), "a colour with alpha");
        var faint = Rich(font);
        faint.color = new Color(1f, 1f, 1f, 0.2f);
        Expect.That(TextLayoutTests.Lay("<color=#00ff00>A</color>", faint).verts[0].color.Equals(new Color32(0, 255, 0, 51)), "a colour without alpha keeps the text colour's alpha");
        var nested = TextLayoutTests.Lay("<color=#ff0000>A<color=#00ff00>B</color>C</color>", Rich(font));
        Expect.That(nested.verts[4].color.Equals(new Color32(0, 255, 0, 255)) && nested.verts[8].color.Equals(new Color32(255, 0, 0, 255)), "an inner colour ends at its own tag");
    }

    private static void BoldAndItalicDrawTheRegularFace(Font font)
    {
        var bold = TextLayoutTests.Lay("<b>A</b>", Rich(font));
        Expect.That(bold.characterCount == 1 && bold.vertexCount == 4, "a bold glyph is one character and one quad");
        TextLayoutTests.ExpectQuad(bold, 0, (0, 10, -16, -2), "bold draws the regular face at the regular place");
        var both = TextLayoutTests.Lay("<i><b>A</b></i>B", Rich(font));
        TextLayoutTests.ExpectQuad(both, 0, (0, 10, -16, -2), "italic and bold together");
        TextLayoutTests.ExpectQuad(both, 1, (12, 22, -16, -2), "and the next glyph follows the regular advance");
    }

    private static void SizeTagsSetTheSizeOfTheirCharacters(Font font)
    {
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("<size=40>A</size>", Rich(font)), 0, (0, 20, -32, -4), "size 40 doubles the ink on its own baseline");
        var scaled = Rich(font);
        (scaled.fontSize, scaled.scaleFactor, scaled.generationExtents) = (10, 2f, new Vector2(60f, 50f));
        TextLayoutTests.ExpectQuad(TextLayoutTests.Lay("<size=20>A</size>", scaled), 0, (0, 20, -32, -4), "a size tag is in pixels before the scale factor");
        var nested = TextLayoutTests.Lay("<size=40>A<size=10>B</size>C</size>", Rich(font));
        TextLayoutTests.ExpectQuad(nested, 2, (30, 50, -32, -4), "after an inner size ends the outer size comes back, past A's 24 and B's 6");
    }

    private static void MixedSizesShareABaselineAndTakeTheLargestLineHeight(Font font)
    {
        var mixed = TextLayoutTests.Lay("A<size=40>B</size>", Rich(font));
        TextLayoutTests.ExpectQuad(mixed, 0, (0, 10, -32, -18), "the small glyph sits on the big glyph's baseline");
        TextLayoutTests.ExpectQuad(mixed, 1, (12, 32, -32, -4), "the big glyph follows the small one's advance");
        Expect.That(mixed.lines[0].height == 40 && Math.Abs(mixed.rectExtents.height - 40f) < 1e-3f, "the line is as tall as its largest size");
        var lines = TextLayoutTests.Lay("A\n<size=40>B</size>", Rich(font));
        Expect.That(lines.lines[0].height == 20 && lines.lines[1].height == 40 && Math.Abs(lines.lines[1].topY + 20f) < 1e-3f, "each line has the height of its own largest size");
        TextLayoutTests.ExpectQuad(lines, 1, (0, 20, -52, -24), "the second line's baseline is its own ascent below its top");
    }

    private static void TagsThatAreNotUnderstoodDrawAsWritten(Font font)
    {
        var literal = new (string Text, int Characters)[]
        {
            ("<u>A</u>", 8), ("<b>A", 4), ("A</b>", 5), ("<size=x>A</size>", 16), ("<size=0>A</size>", 16),
            ("<color=nope>A</color>", 21), ("<color=#ff0000>A", 16), ("<", 1), ("<>", 2), ("<b", 2), ("<b>A</i>", 8),
        };
        foreach (var (text, characters) in literal)
        {
            var generator = TextLayoutTests.Lay(text, Rich(font));
            Expect.That(generator.characterCount == characters && generator.vertexCount == characters * 4 && generator.verts.All(v => v.color.Equals(White)),
                $"'{text}' is {characters} plain characters, got {generator.characterCount}");
        }
        Expect.Equal(1, TextLayoutTests.Lay("<b><i>A</b></i>", Rich(font)).characterCount, "tags that cross still pair by kind");
    }

    private static void WithRichTextOffEveryTagIsText(Font font)
    {
        var plain = TextLayoutTests.Lay("<b>A</b>", TextLayoutTests.Plain(font));
        Expect.That(plain.characterCount == 8 && plain.vertexCount == 32, "with richText off the tags are drawn");
    }

    private static void PreferredSizesSkipTheTags(Font font)
    {
        var generator = new TextGenerator();
        Expect.Near(36f, generator.GetPreferredWidth("<b>AAA</b>", Rich(font)), "preferred width without the tags");
        Expect.Near(40f, generator.GetPreferredHeight("A<size=40>B</size>", Rich(font)), "preferred height of a taller size");
    }
}
