using UnityEngine;

/// <summary>TextGenerator lays out nothing yet, so empty text gives an empty result and any other text is rejected with a
/// NotSupportedException that names the member. UI Text builds one in OnEnable, so construction has to work.</summary>
internal static class TextGeneratorTests
{
    public static void Run()
    {
        ConstructsWithAndWithoutCapacity();
        EmptyTextPopulatesAnEmptyResult();
        TextNeedingLayoutNamesTheMember();
        InvalidateClearsTheResult();
        SettingsCompareByValue();
        Console.WriteLine("PASS: TextGenerator builds, populates empty text, and rejects text it cannot lay out by member name");
    }

    private static TextGenerationSettings Settings(float width = 100f) => new()
    {
        fontSize = 14,
        color = Color.white,
        lineSpacing = 1f,
        scaleFactor = 1f,
        generationExtents = new Vector2(width, 20f),
        pivot = new Vector2(0.5f, 0.5f),
    };

    private static void ConstructsWithAndWithoutCapacity()
    {
        foreach (var generator in new[] { new TextGenerator(), new TextGenerator(50) })
        {
            Expect.Equal(0, generator.vertexCount, "vertexCount of a new generator");
            Expect.Equal(0, generator.characterCount, "characterCount of a new generator");
            Expect.Equal(0, generator.lineCount, "lineCount of a new generator");
            Expect.Equal(0, generator.verts.Count, "verts of a new generator");
            Expect.Equal(0, generator.characters.Count, "characters of a new generator");
            Expect.Equal(0, generator.lines.Count, "lines of a new generator");
        }
        Expect.Throws<ArgumentOutOfRangeException>(() => new TextGenerator(-1), "a negative capacity");
    }

    private static void EmptyTextPopulatesAnEmptyResult()
    {
        var generator = new TextGenerator();
        Expect.That(generator.Populate("", Settings()), "Populate of empty text succeeds");
        Expect.That(generator.PopulateWithErrors("", Settings(), null!), "PopulateWithErrors of empty text succeeds");
        Expect.Equal(0, generator.characterCountVisible, "characterCountVisible");
        Expect.Equal(0, generator.fontSizeUsedForBestFit, "fontSizeUsedForBestFit");
        Expect.Equal(default(Rect), generator.rectExtents, "rectExtents");
        Expect.Equal(0, generator.GetVerticesArray().Length, "GetVerticesArray");
        Expect.Equal(0, generator.GetCharactersArray().Length, "GetCharactersArray");
        Expect.Equal(0, generator.GetLinesArray().Length, "GetLinesArray");
        Expect.Near(0f, generator.GetPreferredWidth("", Settings()), "preferred width of empty text");
        Expect.Near(0f, generator.GetPreferredHeight("", Settings()), "preferred height of empty text");
    }

    private static void TextNeedingLayoutNamesTheMember()
    {
        var generator = new TextGenerator();
        var populate = Expect.Catch<NotSupportedException>(() => generator.Populate("Loading", Settings()), "Populate of text");
        Expect.That(populate.Message.Contains("TextGenerator.Populate"), $"Populate names itself: {populate.Message}");
        var withErrors = Expect.Catch<NotSupportedException>(() => generator.PopulateWithErrors("Loading", Settings(), null!), "PopulateWithErrors of text");
        Expect.That(withErrors.Message.Contains("TextGenerator.PopulateWithErrors"), $"PopulateWithErrors names itself: {withErrors.Message}");
        var width = Expect.Catch<NotSupportedException>(() => generator.GetPreferredWidth("Loading", Settings()), "GetPreferredWidth of text");
        Expect.That(width.Message.Contains("TextGenerator.GetPreferredWidth"), $"GetPreferredWidth names itself: {width.Message}");
        var height = Expect.Catch<NotSupportedException>(() => generator.GetPreferredHeight("Loading", Settings()), "GetPreferredHeight of text");
        Expect.That(height.Message.Contains("TextGenerator.GetPreferredHeight"), $"GetPreferredHeight names itself: {height.Message}");
        Expect.Throws<ArgumentNullException>(() => generator.Populate(null!, Settings()), "null text");
    }

    private static void InvalidateClearsTheResult()
    {
        var generator = new TextGenerator();
        generator.Populate("", Settings());
        generator.Invalidate();
        Expect.Equal(0, generator.vertexCount, "vertexCount after Invalidate");
        Expect.That(generator.Populate("", Settings()), "Populate after Invalidate");
    }

    private static void SettingsCompareByValue()
    {
        Expect.That(Settings().Equals(Settings()), "equal settings compare equal");
        Expect.That(!Settings().Equals(Settings(120f)), "different extents compare unequal");
        var size = Settings();
        size.fontSize = 15;
        Expect.That(!Settings().Equals(size), "different font sizes compare unequal");
        var anchor = Settings();
        anchor.textAnchor = TextAnchor.LowerRight;
        Expect.That(!Settings().Equals(anchor), "different anchors compare unequal");
        Expect.That(Settings().GetHashCode() == Settings().GetHashCode(), "equal settings hash alike");
        Expect.That(!Settings().Equals((object)"settings"), "settings are not equal to another type");
        Expect.That(Settings().Equals((object)Settings()), "boxed equal settings compare equal");
        var font = (Font)Activator.CreateInstance(typeof(Font), System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance, null, [1UL, 1UL], null)!;
        var withFont = Settings();
        withFont.font = font;
        Expect.That(!Settings().Equals(withFont), "a different font compares unequal");
        Expect.That(withFont.Equals(withFont), "the same font compares equal");
        var changes = new Dictionary<string, Func<TextGenerationSettings, TextGenerationSettings>>
        {
            ["color"] = s => { s.color = Color.red; return s; },
            ["lineSpacing"] = s => { s.lineSpacing = 2f; return s; },
            ["richText"] = s => { s.richText = true; return s; },
            ["scaleFactor"] = s => { s.scaleFactor = 2f; return s; },
            ["fontStyle"] = s => { s.fontStyle = FontStyle.Bold; return s; },
            ["alignByGeometry"] = s => { s.alignByGeometry = true; return s; },
            ["resizeTextForBestFit"] = s => { s.resizeTextForBestFit = true; return s; },
            ["resizeTextMinSize"] = s => { s.resizeTextMinSize = 3; return s; },
            ["resizeTextMaxSize"] = s => { s.resizeTextMaxSize = 40; return s; },
            ["updateBounds"] = s => { s.updateBounds = true; return s; },
            ["verticalOverflow"] = s => { s.verticalOverflow = VerticalWrapMode.Overflow; return s; },
            ["horizontalOverflow"] = s => { s.horizontalOverflow = HorizontalWrapMode.Overflow; return s; },
            ["pivot"] = s => { s.pivot = new Vector2(0f, 1f); return s; },
            ["generateOutOfBounds"] = s => { s.generateOutOfBounds = true; return s; },
        };
        foreach (var (field, change) in changes)
            Expect.That(!Settings().Equals(change(Settings())), $"a different {field} compares unequal");
        var info = new UICharInfo { cursorPos = new Vector2(1f, 2f), charWidth = 3f };
        var line = new UILineInfo { startCharIdx = 4, height = 5, topY = 6f, leading = 7f };
        Expect.Near(3f, info.charWidth, "UICharInfo.charWidth");
        Expect.Equal(4, line.startCharIdx, "UILineInfo.startCharIdx");
    }
}
