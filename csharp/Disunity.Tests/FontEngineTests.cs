using UnityEngine;
using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;

/// <summary>
/// FontEngine as TextMesh Pro sees it: no font face ever loads, so every load reports the file as
/// unusable and every query answers as for an unloaded face.
/// </summary>
internal static class FontEngineTests
{
    public static void Run()
    {
        Lifecycle();
        LoadsReportTheUnusableFile();
        FaceQueries();
        GlyphQueries();
        AtlasAdditions();
        LayoutRecords();
        ResetsAtlas();
        Enums();
        Console.WriteLine("PASS: FontEngine loads no face and answers every query as for an unloaded face");
    }

    private static void Lifecycle()
    {
        Expect.Equal(FontEngineError.Success, FontEngine.InitializeFontEngine(), "InitializeFontEngine");
        Expect.Equal(FontEngineError.Success, FontEngine.UnloadFontFace(), "UnloadFontFace with no face");
        Expect.Equal(FontEngineError.Success, FontEngine.UnloadAllFontFaces(), "UnloadAllFontFaces");
        Expect.Equal(FontEngineError.Success, FontEngine.DestroyFontEngine(), "DestroyFontEngine");
    }

    private static void LoadsReportTheUnusableFile()
    {
        const FontEngineError unusable = FontEngineError.Invalid_File_Format;
        var font = new Font(1, 1);
        var loads = new (string Overload, FontEngineError Result)[]
        {
            ("path", FontEngine.LoadFontFace("a.ttf")),
            ("path, size", FontEngine.LoadFontFace("a.ttf", 90.5f)),
            ("path, size, index", FontEngine.LoadFontFace("a.ttf", 90.5f, 1)),
            ("family, style", FontEngine.LoadFontFace("Arial", "Bold")),
            ("family, style, size", FontEngine.LoadFontFace("Arial", "Bold", 90.5f)),
            ("bytes", FontEngine.LoadFontFace(new byte[] { 0, 1, 0, 0 })),
            ("bytes, size", FontEngine.LoadFontFace(new byte[] { 0, 1, 0, 0 }, 90.5f)),
            ("bytes, size, index", FontEngine.LoadFontFace(new byte[] { 0, 1, 0, 0 }, 90.5f, 1)),
            ("font", FontEngine.LoadFontFace(font)),
            ("font, size", FontEngine.LoadFontFace(font, 90.5f)),
            ("font, size, index", FontEngine.LoadFontFace(font, 90.5f, 1)),
        };
        foreach (var (overload, result) in loads)
            Expect.Equal(unusable, result, "LoadFontFace(" + overload + ")");
        Expect.Equal(FontEngineError.Invalid_Face, FontEngine.SetFaceSize(90), "SetFaceSize after failed loads");
        Expect.That(FontEngine.GetFaceInfo().Equals(default(FaceInfo)), "a failed load leaves no face");
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "a failed load leaves no glyphs");
    }

    private static void FaceQueries()
    {
        var info = FontEngine.GetFaceInfo();
        Expect.That(info.familyName == null && info.pointSize == 0 && info.lineHeight == 0 && info.unitsPerEM == 0, "GetFaceInfo is the default face");
        Expect.Equal(0, FontEngine.GetSystemFontNames().Length, "GetSystemFontNames");
        Expect.Equal(0, FontEngine.GetSystemFontReferences().Length, "GetSystemFontReferences");
        Expect.That(!FontEngine.TryGetSystemFontReference("Arial", "Bold", out var reference) && reference.familyName == null, "TryGetSystemFontReference");
    }

    private static void GlyphQueries()
    {
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "GetGlyphIndex");
        Expect.That(!FontEngine.TryGetGlyphIndex('A', out var index) && index == 0, "TryGetGlyphIndex");
        Expect.That(!FontEngine.TryGetGlyphWithUnicodeValue('A', GlyphLoadFlags.LOAD_DEFAULT, out var byUnicode) && byUnicode == null,
            "TryGetGlyphWithUnicodeValue");
        Expect.That(!FontEngine.TryGetGlyphWithIndexValue(36, GlyphLoadFlags.LOAD_RENDER, out var byIndex) && byIndex == null,
            "TryGetGlyphWithIndexValue");
    }

    private static void AtlasAdditions()
    {
        var free = new List<GlyphRect> { new(0, 0, 64, 64) };
        var used = new List<GlyphRect>();
        var texture = new Texture2D(64, 64);
        var added = FontEngine.TryAddGlyphToTexture(36, 2, GlyphPackingMode.BestShortSideFit, free, used, GlyphRenderMode.SDFAA, texture, out var one);
        Expect.That(!added && one == null, "TryAddGlyphToTexture adds no glyph");
        var many = FontEngine.TryAddGlyphsToTexture(new List<uint> { 36, 37 }, 2, GlyphPackingMode.BestAreaFit, free, used, GlyphRenderMode.SDF, texture, out var glyphs);
        Expect.That(!many && glyphs.Length == 0, "TryAddGlyphsToTexture with indexes adds no glyph");
        var toAdd = new List<Glyph> { new() };
        var done = new List<Glyph> { new() };
        Expect.That(!FontEngine.TryAddGlyphsToTexture(toAdd, done, 2, GlyphPackingMode.BestLongSideFit, free, used, GlyphRenderMode.SMOOTH, texture),
            "TryAddGlyphsToTexture with glyphs adds none");
        Expect.That(done.Count == 0 && toAdd.Count == 1, "the added list is emptied and the input is left alone");
        Expect.That(free.Count == 1 && free[0] == new GlyphRect(0, 0, 64, 64) && used.Count == 0, "no atlas space is taken");
    }

    private static void LayoutRecords()
    {
        Expect.Equal(0, FontEngine.GetGlyphPairAdjustmentTable([36, 37]).Length, "pair adjustments");
        Expect.Equal(0, FontEngine.GetMarkToBaseAdjustmentRecords(36).Length, "mark to base records");
        Expect.Equal(0, FontEngine.GetMarkToMarkAdjustmentRecords(36).Length, "mark to mark records");
        Expect.Equal(0, FontEngine.GetLigatureSubstitutionRecords(36).Length, "ligature records");
    }

    private static void ResetsAtlas()
    {
        var texture = new Texture2D(2, 2);
        texture.SetPixel(1, 1, Color.red);
        FontEngine.ResetAtlasTexture(texture);
        Expect.That(texture.GetPixel(1, 1) == Color.red, "ResetAtlasTexture leaves a texture nothing was drawn into as it is");
        Expect.Catch<ArgumentNullException>(() => FontEngine.ResetAtlasTexture(null!), "ResetAtlasTexture(null)");
    }

    private static void Enums()
    {
        Expect.That((int)FontEngineError.Success == 0 && (int)FontEngineError.Invalid_File_Format == 2 && (int)FontEngineError.Invalid_Face == 35,
            "FontEngineError values");
        Expect.That((int)GlyphRenderMode.SDFAA == 4165 && (int)GlyphRenderMode.SDF == 4134 && (int)GlyphRenderMode.SMOOTH == 4117
            && (int)GlyphRenderMode.RASTER_HINTED == 4122, "GlyphRenderMode values");
        Expect.That((int)GlyphPackingMode.BestShortSideFit == 0 && (int)GlyphPackingMode.ContactPointRule == 4, "GlyphPackingMode values");
        Expect.That((int)GlyphLoadFlags.LOAD_RENDER == 4 && (int)GlyphLoadFlags.LOAD_NO_HINTING == 2
            && (int)GlyphLoadFlags.LOAD_COLOR == 1048576, "GlyphLoadFlags values");
        Expect.That((int)FontFeatureLookupFlags.IgnoreLigatures == 4 && (int)FontFeatureLookupFlags.IgnoreSpacingAdjustments == 256,
            "FontFeatureLookupFlags values");
    }
}
