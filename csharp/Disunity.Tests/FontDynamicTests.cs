using System.Reflection;
using Disunity.Hosting;
using UnityEngine;

/// <summary>Font.dynamic is true for a font the engine can draw glyphs from on demand, which is a font that holds its font
/// file, and false for a font with no file. UI Text reads it while it builds its generation settings.</summary>
internal static class FontDynamicTests
{
    public static void Run()
    {
        TheGetterHasTheShapeGameCodeBindsTo();
        DisunityHost.LoadWorld();
        try
        {
            AFontWithItsFileIsDynamic();
            AFontWithoutAFileIsNot();
            TextReadsItWhileBuildingGenerationSettings();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: Font.dynamic is true for a font with a font file and false for one without");
    }

    private static void TheGetterHasTheShapeGameCodeBindsTo()
    {
        var getter = typeof(Font).GetProperty("dynamic", BindingFlags.Public | BindingFlags.Instance)?.GetMethod
            ?? throw new InvalidOperationException("Font has no public instance dynamic getter");
        Expect.That(getter.Name == "get_dynamic" && getter.ReturnType == typeof(bool) && getter.GetParameters().Length == 0,
            "Font.get_dynamic() returns Boolean");
    }

    private static void AFontWithItsFileIsDynamic()
    {
        Expect.That(DisunityHost.RegisterFont(SyntheticFont.Bytes()).dynamic, "a registered font keeps its file");
        Expect.That(new Font("Serif", [1, 2, 3]).dynamic, "a player font with a file");
    }

    private static void AFontWithoutAFileIsNot()
    {
        Expect.That(!new Font("Atlas", []).dynamic, "a player font with an empty file");
        Expect.That(!new Font(7, DisunityHost.WorldHandle).dynamic, "a font handle with no file");
    }

    // The same reads UI Text makes: the font, then whether it is dynamic, which decides whether the requested size applies.
    private static TextGenerationSettings GenerationSettingsFor(Font font)
    {
        var isDynamic = font.dynamic;
        return new TextGenerationSettings
        {
            font = font,
            fontSize = isDynamic ? 14 : 0,
            lineSpacing = 1f,
            scaleFactor = 1f,
        };
    }

    private static void TextReadsItWhileBuildingGenerationSettings()
    {
        var withFile = GenerationSettingsFor(new Font("Serif", [1, 2, 3]));
        var atlasOnly = GenerationSettingsFor(new Font("Atlas", []));
        Expect.Equal(14, withFile.fontSize, "a dynamic font takes the requested size");
        Expect.Equal(0, atlasOnly.fontSize, "an atlas-only font does not");
        Expect.That(!withFile.Equals(atlasOnly), "the settings of the two fonts differ");
        Expect.That(new TextGenerator().Populate("", withFile), "the generator accepts the settings");
    }
}
