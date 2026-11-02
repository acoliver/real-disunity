using Disunity.Hosting;
using UnityEngine;
using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;

/// <summary>
/// FontEngine as TextMesh Pro sees it: it loads a face from font bytes, reads face and glyph measurements, and
/// packs and draws glyphs into an Alpha8 atlas texture. The synthetic font has one drawn glyph (a right triangle,
/// 500 by 700 units in 1000 per em, advance 600) that every code point from space to tilde maps to.
/// </summary>
internal static class FontEngineTests
{
    private const GlyphRenderMode Sdf = GlyphRenderMode.SDFAA;

    public static void Run()
    {
        WithoutAWorldNothingLoads();
        DisunityHost.LoadWorld();
        try
        {
            WithoutAFaceQueriesAnswerAsUnloaded();
            LoadOverloads();
            FaceInfoScalesWithSize();
            GlyphQueries();
            AddsGlyphsToAnAtlas();
            AddsGlyphListsAndKeepsWhatDoesNotFit();
            EveryPackingModePlacesWithoutOverlap();
            ModesPickTheirSlot();
            LargeFieldsAndShortSlotsAddNothing();
            RefusesUnsupportedAtlases();
            ResetsAtlas();
            FaceEndsWithTheWorld();
        }
        finally { DisunityHost.UnloadWorld(); }
        Enums();
        Console.WriteLine("PASS: FontEngine loads faces from bytes and draws packed glyphs into atlas textures");
    }

    private static void WithoutAWorldNothingLoads()
    {
        Expect.Equal(FontEngineError.Invalid_Library, FontEngine.LoadFontFace(SyntheticFont.Bytes(), 20, 0), "LoadFontFace with no world");
        Expect.Equal(FontEngineError.Success, FontEngine.InitializeFontEngine(), "InitializeFontEngine");
        Expect.Equal(FontEngineError.Success, FontEngine.UnloadFontFace(), "UnloadFontFace with no face");
        Expect.Equal(FontEngineError.Success, FontEngine.UnloadAllFontFaces(), "UnloadAllFontFaces");
        Expect.Equal(FontEngineError.Success, FontEngine.DestroyFontEngine(), "DestroyFontEngine");
    }

    private static void WithoutAFaceQueriesAnswerAsUnloaded()
    {
        Expect.Equal(FontEngineError.Invalid_Face, FontEngine.SetFaceSize(90), "SetFaceSize with no face");
        Expect.That(FontEngine.GetFaceInfo().Equals(default(FaceInfo)), "GetFaceInfo with no face is the default face");
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "GetGlyphIndex with no face");
        Expect.That(!FontEngine.TryGetGlyphIndex('A', out var index) && index == 0, "TryGetGlyphIndex with no face");
        Expect.That(!FontEngine.TryGetGlyphWithUnicodeValue('A', GlyphLoadFlags.LOAD_DEFAULT, out var byUnicode) && byUnicode == null, "TryGetGlyphWithUnicodeValue with no face");
        Expect.That(!FontEngine.TryGetGlyphWithIndexValue(1, GlyphLoadFlags.LOAD_RENDER, out var byIndex) && byIndex == null, "TryGetGlyphWithIndexValue with no face");
        Expect.Equal(0, FontEngine.GetGlyphPairAdjustmentTable([1, 2]).Length, "pair adjustments with no face");
        var texture = Atlas(64);
        var free = new List<GlyphRect> { new(0, 0, 64, 64) };
        var used = new List<GlyphRect>();
        Expect.That(!FontEngine.TryAddGlyphToTexture(1, 2, GlyphPackingMode.BestShortSideFit, free, used, Sdf, texture, out var glyph) && glyph == null,
            "TryAddGlyphToTexture with no face adds nothing");
        Expect.That(used.Count == 0 && free.Count == 1 && free[0] == new GlyphRect(0, 0, 64, 64), "no atlas space is taken with no face");
        Expect.Equal(0, FontEngine.GetSystemFontNames().Length, "GetSystemFontNames");
        Expect.Equal(0, FontEngine.GetSystemFontReferences().Length, "GetSystemFontReferences");
        Expect.That(!FontEngine.TryGetSystemFontReference("Arial", "Bold", out var reference) && reference.familyName == null, "TryGetSystemFontReference");
    }

    private static void LoadOverloads()
    {
        var bytes = SyntheticFont.Bytes();
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(bytes), "bytes");
        Expect.Near(16, FontEngine.GetFaceInfo().pointSize, "a face loaded without a size is 16 points");
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(bytes, 90.5f), "bytes, size");
        Expect.Near(90.5f, FontEngine.GetFaceInfo().pointSize, "the point size");
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(bytes, 40, 0), "bytes, size, index");
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace(bytes, 40, 3), "a face index past the file's faces");
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "a failed load unloads the previous face");
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace([0, 1, 0, 0]), "bytes that are not a font");
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace(Array.Empty<byte>()), "no bytes");
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace((byte[])null!), "null bytes");
        Expect.Equal(FontEngineError.Invalid_Pixel_Size, FontEngine.LoadFontFace(bytes, 0), "a zero size");
        Expect.Equal(FontEngineError.Invalid_Pixel_Size, FontEngine.LoadFontFace(bytes, float.NaN), "a NaN size");
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace("Arial", "Bold"), "family and style");
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace("Arial", "Bold", 20), "family, style, size");
        LoadFromFileAndFont(bytes);
    }

    private static void LoadFromFileAndFont(byte[] bytes)
    {
        var path = Path.Combine(Path.GetTempPath(), $"font-engine-{Environment.ProcessId}.ttf");
        File.WriteAllBytes(path, bytes);
        try
        {
            Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(path, 30), "path, size");
            Expect.Equal(1u, FontEngine.GetGlyphIndex('A'), "a face from a file");
            Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(path), "path");
            Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(path, 30, 0), "path, size, index");
        }
        finally { File.Delete(path); }
        Expect.Equal(FontEngineError.Invalid_File_Path, FontEngine.LoadFontFace(path), "a file that is gone");
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "a failed path load unloads the previous face");
        var withData = DisunityHost.RegisterFont(bytes);
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(withData), "a font with its file data");
        Expect.Equal(1u, FontEngine.GetGlyphIndex('A'), "the font's own face");
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(withData, 33), "font, size");
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(withData, 33, 0), "font, size, index");
        var withoutData = new Font(7, DisunityHost.WorldHandle);
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace(withoutData), "a font with no data is unusable");
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "an unusable font unloads the previous face");
        Expect.Equal(FontEngineError.Invalid_File_Format, FontEngine.LoadFontFace(withoutData, 20, 0), "a font with no data, size, index");
        Expect.Throws<ArgumentNullException>(() => FontEngine.LoadFontFace((Font)null!), "a null font");
    }

    private static void FaceInfoScalesWithSize()
    {
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(SyntheticFont.Bytes(), 100), "load");
        var info = FontEngine.GetFaceInfo();
        Expect.That(info.unitsPerEM == 1000 && info.faceIndex == 0 && info.scale == 1 && info.baseline == 0, "face identity");
        Expect.Near(100, info.pointSize, "pointSize");
        Expect.Near(80, info.ascentLine, "ascentLine");
        Expect.Near(-20, info.descentLine, "descentLine");
        Expect.Near(100, info.lineHeight, "lineHeight");
        Expect.Near(60, info.tabWidth, "tabWidth is the advance of a space");
        Expect.That(info.capLine > 0 && info.meanLine > 0 && info.underlineThickness > 0 && info.strikethroughThickness > 0, "fallback lines");
        Expect.That(info.superscriptSize > 0 && info.subscriptSize > 0 && info.superscriptOffset > 0 && info.subscriptOffset < 0, "script fallbacks");
        Expect.Equal(string.Empty, info.familyName, "a font with no name table has no family name");
        Expect.Equal(FontEngineError.Success, FontEngine.SetFaceSize(50), "SetFaceSize");
        var half = FontEngine.GetFaceInfo();
        Expect.Near(50, half.pointSize, "pointSize after SetFaceSize");
        Expect.Near(info.ascentLine / 2, half.ascentLine, "ascentLine scales with the size");
        Expect.Near(info.lineHeight / 2, half.lineHeight, "lineHeight scales with the size");
        Expect.Equal(FontEngineError.Invalid_Pixel_Size, FontEngine.SetFaceSize(0), "SetFaceSize(0)");
        Expect.Equal(FontEngineError.Success, FontEngine.UnloadFontFace(), "UnloadFontFace");
        Expect.Equal(FontEngineError.Invalid_Face, FontEngine.SetFaceSize(50), "SetFaceSize after unload");
    }

    private static void GlyphQueries()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 100);
        Expect.Equal(1u, FontEngine.GetGlyphIndex('A'), "GetGlyphIndex");
        Expect.Equal(0u, FontEngine.GetGlyphIndex(0x4E00), "an unmapped code point has glyph 0");
        Expect.That(FontEngine.TryGetGlyphIndex('A', out var index) && index == 1, "TryGetGlyphIndex");
        Expect.That(!FontEngine.TryGetGlyphIndex(0x4E00, out index) && index == 0, "TryGetGlyphIndex for an unmapped code point");
        Expect.That(FontEngine.TryGetGlyphWithUnicodeValue('A', GlyphLoadFlags.LOAD_DEFAULT, out var glyph) && glyph != null, "TryGetGlyphWithUnicodeValue");
        var metrics = glyph!.metrics;
        Expect.That(glyph.index == 1 && glyph.glyphRect == GlyphRect.zero && glyph.scale == 1 && glyph.atlasIndex == 0, "a glyph with no atlas place");
        Expect.That(Math.Abs(metrics.width - 50) < 1e-3 && Math.Abs(metrics.height - 70) < 1e-3 && Math.Abs(metrics.horizontalAdvance - 60) < 1e-3, "metrics at 100 points");
        Expect.That(Math.Abs(metrics.horizontalBearingX) < 1e-3 && Math.Abs(metrics.horizontalBearingY - 70) < 1e-3, "bearings");
        FontEngine.SetFaceSize(50);
        FontEngine.TryGetGlyphWithIndexValue(1, GlyphLoadFlags.LOAD_RENDER, out var small);
        Expect.Near(25, small!.metrics.width, "metrics scale with the size", 1e-3f);
        Expect.That(!FontEngine.TryGetGlyphWithUnicodeValue(0x4E00, GlyphLoadFlags.LOAD_DEFAULT, out var none) && none == null, "no glyph for an unmapped code point");
        Expect.That(!FontEngine.TryGetGlyphWithIndexValue(99, GlyphLoadFlags.LOAD_DEFAULT, out none) && none == null, "no glyph past the font's glyphs");
        Expect.Equal(0, FontEngine.GetGlyphPairAdjustmentTable([1, 1]).Length, "a font with no kern table has no pair adjustments");
        Expect.Equal(0, FontEngine.GetGlyphPairAdjustmentTable([]).Length, "no glyphs, no pairs");
        Expect.Throws<ArgumentNullException>(() => FontEngine.GetGlyphPairAdjustmentTable(null!), "null glyph indexes");
        Expect.Throws<ArgumentNullException>(() => FontEngine.GetPairAdjustmentRecords(null!), "null glyph index list");
        Expect.Equal(0, FontEngine.GetPairAdjustmentRecords([1, 1]).Length, "the list form of a font with no kern table");
        Expect.Equal(0, FontEngine.GetMarkToBaseAdjustmentRecords([1, 0]).Length, "no mark-to-base records for a list");
        Expect.Equal(0, FontEngine.GetMarkToMarkAdjustmentRecords([1, 0]).Length, "no mark-to-mark records for a list");
        Expect.Equal(0, FontEngine.GetLigatureSubstitutionRecords([1, 0]).Length, "no ligature records for a list");
        Expect.Throws<ArgumentNullException>(() => FontEngine.GetMarkToBaseAdjustmentRecords((List<uint>)null!), "null mark-to-base list");
        Expect.Throws<ArgumentNullException>(() => FontEngine.GetMarkToMarkAdjustmentRecords((List<uint>)null!), "null mark-to-mark list");
        Expect.Throws<ArgumentNullException>(() => FontEngine.GetLigatureSubstitutionRecords((List<uint>)null!), "null ligature list");
        KernPairs();
        Expect.Equal(0, FontEngine.GetMarkToBaseAdjustmentRecords(1).Length, "mark to base records");
        Expect.Equal(0, FontEngine.GetMarkToMarkAdjustmentRecords(1).Length, "mark to mark records");
        Expect.Equal(0, FontEngine.GetLigatureSubstitutionRecords(1).Length, "ligature records");
    }

    private static void KernPairs()
    {
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(SyntheticFont.Bytes([(1, 1, -100), (0, 1, 30)]), 50), "a font with a kern table");
        var records = FontEngine.GetGlyphPairAdjustmentTable([1, 0, 1]);
        Expect.Equal(2, records.Length, "the kerned pairs among the glyphs asked about, each pair once");
        var pair = records.Single(r => r.firstAdjustmentRecord.glyphIndex == 1 && r.secondAdjustmentRecord.glyphIndex == 1);
        Expect.Near(-5, pair.firstAdjustmentRecord.glyphValueRecord.xAdvance, "the kerning in pixels at 50 points", 1e-3f);
        Expect.That(pair.secondAdjustmentRecord.glyphValueRecord == default, "the second glyph is not moved");
        var other = records.Single(r => r.firstAdjustmentRecord.glyphIndex == 0);
        Expect.Near(1.5f, other.firstAdjustmentRecord.glyphValueRecord.xAdvance, "a positive adjustment", 1e-3f);
        Expect.Equal(2, FontEngine.GetPairAdjustmentRecords([0, 1]).Length, "the list form finds the same pairs");
        Expect.Equal(1, FontEngine.GetPairAdjustmentRecords([1]).Length, "a single glyph kerns with itself");
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 100);
    }

    private static Texture2D Atlas(int side) => new(side, side, TextureFormat.Alpha8, false);

    private static int NonZero(Texture2D texture) => texture.GetRawTextureData().Count(b => b != 0);

    private static byte Pixel(Texture2D texture, int x, int y) => texture.GetRawTextureData()[y * texture.width + x];

    private static bool Overlap(GlyphRect a, GlyphRect b) =>
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

    private static void AddsGlyphsToAnAtlas()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 100);
        var texture = Atlas(128);
        var before = texture.updateCount;
        var free = new List<GlyphRect> { new(0, 0, 128, 128) };
        var used = new List<GlyphRect>();
        Expect.That(FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.BestShortSideFit, free, used, Sdf, texture, out var glyph), "TryAddGlyphToTexture");
        var rect = used[0];
        Expect.That(used.Count == 1 && rect.width == 58 && rect.height == 78, $"the used rectangle is the glyph plus the padding on each side: {rect.width}x{rect.height}");
        Expect.That(glyph!.glyphRect == new GlyphRect(rect.x + 4, rect.y + 4, 50, 70), $"the glyph's own rectangle is the glyph without the padding: {glyph.glyphRect.x},{glyph.glyphRect.y} {glyph.glyphRect.width}x{glyph.glyphRect.height}");
        Expect.That(free.Count > 0 && free.All(f => !Overlap(f, rect) && f.x >= 0 && f.y >= 0 && f.x + f.width <= 128 && f.y + f.height <= 128), "the free rectangles stay inside the atlas and clear of the glyph");
        Expect.Near(60, glyph.metrics.horizontalAdvance, "the glyph keeps its metrics", 1e-3f);
        Expect.That(texture.updateCount > before, "the texture is applied so the renderer sees the glyph");
        Expect.That(NonZero(texture) > 1000, "the glyph is drawn into the texture");
        var left = glyph.glyphRect.x;
        var bottom = glyph.glyphRect.y;
        Expect.That(Pixel(texture, left + 4, bottom + 4) > 200, "the field is high inside the outline");
        Expect.That(Pixel(texture, left + 45, bottom + 65) < 40, "the field is low outside the outline");
        Expect.That(Pixel(texture, left, bottom + 20) is >= 100 and <= 160, "the field is about one half at the edge");
        Expect.That(Pixel(texture, 120, 120) == 0, "pixels outside the glyph rectangle are untouched");
        var coverage = Atlas(128);
        FontEngine.TryAddGlyphToTexture(1, 2, GlyphPackingMode.BestShortSideFit, [new GlyphRect(0, 0, 128, 128)], new List<GlyphRect>(), GlyphRenderMode.SMOOTH, coverage, out var plain);
        Expect.That(Pixel(coverage, plain!.glyphRect.x + 3, plain.glyphRect.y + 3) > 240 && Pixel(coverage, plain.glyphRect.x - 2, plain.glyphRect.y - 2) == 0, "a coverage glyph is solid inside and empty in the padding");
        Expect.That(FontEngine.TryAddGlyphToTexture(0, 4, GlyphPackingMode.BestShortSideFit, free, used, Sdf, texture, out var empty)
            && empty!.glyphRect == GlyphRect.zero && used.Count == 1, "a glyph with no outline takes no space");
        Expect.That(!FontEngine.TryAddGlyphToTexture(99, 4, GlyphPackingMode.BestShortSideFit, free, used, Sdf, texture, out var missing) && missing == null && used.Count == 1, "no such glyph");
        var tiny = Atlas(32);
        var tinyFree = new List<GlyphRect> { new(0, 0, 32, 32) };
        Expect.That(!FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.BestShortSideFit, tinyFree, new List<GlyphRect>(), Sdf, tiny, out var none) && none == null
            && NonZero(tiny) == 0 && tinyFree.Count == 1 && tinyFree[0] == new GlyphRect(0, 0, 32, 32), "a glyph that does not fit changes nothing");
    }

    private static void AddsGlyphListsAndKeepsWhatDoesNotFit()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 40);
        var texture = Atlas(64);
        var free = new List<GlyphRect> { new(0, 0, 64, 64) };
        var used = new List<GlyphRect>();
        var all = FontEngine.TryAddGlyphsToTexture(new List<uint> { 1, 0, 1, 1 }, 2, GlyphPackingMode.BestAreaFit, free, used, Sdf, texture, out var glyphs);
        Expect.That(all && glyphs.Length == 4 && used.Count == 3, $"four glyphs, one with no outline, are added: {glyphs.Length}, {used.Count}");
        Expect.That(glyphs[1].glyphRect == GlyphRect.zero, "the empty glyph has the empty rectangle");
        Expect.That(glyphs[0].glyphRect == new GlyphRect(used[0].x + 2, used[0].y + 2, used[0].width - 4, used[0].height - 4), "a listed glyph's rectangle is inside its used rectangle by the padding");
        for (var i = 0; i < used.Count; i++)
            for (var j = i + 1; j < used.Count; j++) Expect.That(!Overlap(used[i], used[j]), "used rectangles do not overlap");
        Expect.That(NonZero(texture) > 100, "the glyphs are drawn");
        Expect.That(used.Any(r => r.y > 0) && glyphs.Where(g => g.glyphRect.width > 0).All(g => Pixel(texture, g.glyphRect.x + 3, g.glyphRect.y + 3) > 200), "every glyph is drawn inside its own rectangle, wherever it was placed");
        var crowded = Atlas(64);
        var crowdedFree = new List<GlyphRect> { new(0, 0, 64, 64) };
        var crowdedUsed = new List<GlyphRect>();
        var partial = FontEngine.TryAddGlyphsToTexture(new List<uint> { 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1 }, 2, GlyphPackingMode.BestShortSideFit, crowdedFree, crowdedUsed, Sdf, crowded, out var some);
        Expect.That(!partial && some.Length == crowdedUsed.Count && some.Length is > 0 and < 12, $"a full atlas adds what fits and reports failure: {some.Length}");
        var toAdd = new List<Glyph> { new() { index = 1 }, new() { index = 1 }, new() { index = 77 }, new() { index = 1 } };
        var added = new List<Glyph> { new() };
        var texture2 = Atlas(64);
        var done = FontEngine.TryAddGlyphsToTexture(toAdd, added, 2, GlyphPackingMode.ContactPointRule, [new GlyphRect(0, 0, 64, 64)], new List<GlyphRect>(), Sdf, texture2);
        Expect.That(!done && added.Count == 3 && toAdd.Count == 1 && toAdd[0].index == 77, "added glyphs move over, the glyph that cannot be drawn stays");
        Expect.That(added.All(g => g.glyphRect.width == 20 && g.glyphRect.height == 28 && g.metrics.width > 0), "the added glyphs carry their rectangle and metrics");
        Expect.Throws<ArgumentNullException>(() => FontEngine.TryAddGlyphsToTexture(null!, added, 2, GlyphPackingMode.BestShortSideFit, free, used, Sdf, texture2), "null glyphs to add");
        Expect.Throws<ArgumentNullException>(() => FontEngine.TryAddGlyphsToTexture(new List<uint>(), 2, GlyphPackingMode.BestShortSideFit, free, used, Sdf, null!, out _), "null texture");
        Expect.Throws<ArgumentOutOfRangeException>(() => FontEngine.TryAddGlyphsToTexture(new List<uint>(), -1, GlyphPackingMode.BestShortSideFit, free, used, Sdf, texture2, out _), "negative padding");
    }

    private static void EveryPackingModePlacesWithoutOverlap()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 30);
        foreach (var mode in Enum.GetValues<GlyphPackingMode>())
        {
            var texture = Atlas(96);
            var free = new List<GlyphRect> { new(0, 0, 96, 96) };
            var used = new List<GlyphRect>();
            FontEngine.TryAddGlyphsToTexture(new List<uint> { 1, 1, 1, 1, 1, 1 }, 1, mode, free, used, GlyphRenderMode.SMOOTH, texture, out var glyphs);
            Expect.Equal(6, glyphs.Length, $"{mode}: all glyphs fit");
            Expect.That(used.All(r => r.x >= 0 && r.y >= 0 && r.x + r.width <= 96 && r.y + r.height <= 96), $"{mode}: inside the atlas");
            for (var i = 0; i < used.Count; i++)
                for (var j = i + 1; j < used.Count; j++) Expect.That(!Overlap(used[i], used[j]), $"{mode}: used rectangles do not overlap");
            Expect.That(free.All(f => used.All(u => !Overlap(f, u))), $"{mode}: free space excludes used space");
        }
        var bottomLeft = new List<GlyphRect> { new(0, 0, 96, 96) };
        var bottomLeftUsed = new List<GlyphRect>();
        FontEngine.TryAddGlyphToTexture(1, 1, GlyphPackingMode.BottomLeftRule, bottomLeft, bottomLeftUsed, GlyphRenderMode.SMOOTH, Atlas(96), out var first);
        Expect.That(first!.glyphRect.x == 1 && first.glyphRect.y == 1 && bottomLeftUsed[0].x == 0 && bottomLeftUsed[0].y == 0, "the first rectangle goes to the corner");
    }

    private static GlyphRect Place(GlyphPackingMode mode, params GlyphRect[] slots)
    {
        var free = new List<GlyphRect>(slots);
        var used = new List<GlyphRect>();
        var atlas = Atlas(512);
        Expect.That(FontEngine.TryAddGlyphToTexture(1, 4, mode, free, used, GlyphRenderMode.SMOOTH, atlas, out var glyph), $"{mode}: the glyph fits");
        var rect = used[0];
        Expect.That(InkInside(atlas, rect), $"{mode}: the glyph is drawn inside its rectangle and nowhere else");
        Expect.That(used.Count == 1 && glyph!.glyphRect == new GlyphRect(rect.x + 4, rect.y + 4, rect.width - 8, rect.height - 8), $"{mode}: the placed rectangle is recorded");
        Expect.That(free.All(f => !Overlap(f, rect)), $"{mode}: free space excludes the glyph");
        Expect.That(free.Select((a, i) => free.Where((b, j) => i != j && a.x >= b.x && a.y >= b.y && a.x + a.width <= b.x + b.width && a.y + a.height <= b.y + b.height).Any())
            .All(contained => !contained), $"{mode}: no free rectangle lies inside another");
        return rect;
    }

    private static bool InkInside(Texture2D atlas, GlyphRect rect)
    {
        var pixels = atlas.GetRawTextureData();
        var inside = false;
        for (var y = 0; y < atlas.height; y++)
            for (var x = 0; x < atlas.width; x++)
            {
                if (pixels[y * atlas.width + x] == 0) continue;
                if (x < rect.x || y < rect.y || x >= rect.x + rect.width || y >= rect.y + rect.height) return false;
                inside = true;
            }
        return inside;
    }

    private static void ModesPickTheirSlot()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 100);
        var roomy = new GlyphRect(0, 100, 400, 400);
        var snug = new GlyphRect(420, 200, 60, 80);
        var tall = new GlyphRect(100, 0, 300, 90);
        foreach (var mode in new[] { GlyphPackingMode.BestShortSideFit, GlyphPackingMode.BestLongSideFit, GlyphPackingMode.BestAreaFit })
            Expect.That(Place(mode, roomy, snug).x == 420, $"{mode}: the slot with the least waste wins");
        Expect.Equal(100, Place(GlyphPackingMode.BottomLeftRule, roomy, tall).x, "BottomLeftRule: the lowest placement wins");
        Expect.Equal(0, Place(GlyphPackingMode.BottomLeftRule, new GlyphRect(0, 0, 100, 100), new GlyphRect(200, 0, 100, 100)).x, "BottomLeftRule: the left one wins a tie");
        var corner = Place(GlyphPackingMode.ContactPointRule, new GlyphRect(100, 100, 200, 200), new GlyphRect(0, 0, 200, 200));
        Expect.That(corner.x == 0 && corner.y == 0, "ContactPointRule: the corner of the atlas wins");
        var free = new List<GlyphRect> { new(0, 0, 512, 512) };
        var used = new List<GlyphRect>();
        for (var i = 0; i < 3; i++)
            FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.ContactPointRule, free, used, GlyphRenderMode.SMOOTH, Atlas(512), out _);
        Expect.That(used.Skip(1).All(u => used.Take(1).Any(first => u.x == first.x + first.width || u.y == first.y + first.height || u.x + u.width == first.x)),
            "ContactPointRule: later rectangles touch the first");
    }

    private static void LargeFieldsAndShortSlotsAddNothing()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 400);
        var free = new List<GlyphRect> { new(0, 0, 512, 512) };
        var used = new List<GlyphRect>();
        var texture = Atlas(512);
        Expect.That(!FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.BestShortSideFit, free, used, Sdf, texture, out var none) && none == null,
            "a distance field larger than 256 pixels is not drawn");
        Expect.That(used.Count == 0 && free.Count == 1 && free[0] == new GlyphRect(0, 0, 512, 512) && NonZero(texture) == 0, "and it takes no space");
        Expect.That(FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.BestShortSideFit, free, used, GlyphRenderMode.SMOOTH, texture, out _), "the same glyph as coverage is drawn");
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 100);
        var squat = new List<GlyphRect> { new(0, 0, 400, 20) };
        Expect.That(!FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.BestShortSideFit, squat, new List<GlyphRect>(), Sdf, Atlas(512), out _), "a slot that is wide enough but too short");
        var narrow = new List<GlyphRect> { new(0, 0, 20, 400) };
        Expect.That(!FontEngine.TryAddGlyphToTexture(1, 4, GlyphPackingMode.BestShortSideFit, narrow, new List<GlyphRect>(), Sdf, Atlas(512), out _), "a slot that is tall enough but too narrow");
    }

    private static void RefusesUnsupportedAtlases()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 30);
        var free = new List<GlyphRect> { new(0, 0, 32, 32) };
        var used = new List<GlyphRect>();
        var color = new Texture2D(32, 32, TextureFormat.RGBA32, false);
        var error = Expect.Catch<NotSupportedException>(() => FontEngine.TryAddGlyphToTexture(1, 1, GlyphPackingMode.BestShortSideFit, free, used, Sdf, color, out _), "an RGBA32 atlas");
        Expect.That(error.Message.Contains("TryAddGlyphToTexture"), "the message names the member");
        Expect.Throws<NotSupportedException>(() => FontEngine.TryAddGlyphToTexture(1, 1, GlyphPackingMode.BestShortSideFit, free, used, GlyphRenderMode.COLOR, Atlas(32), out _), "color glyphs");
        Expect.That(used.Count == 0 && NonZero(color) == 0, "a refused atlas is left alone");
    }

    private static void ResetsAtlas()
    {
        var texture = Atlas(8);
        var raw = texture.GetRawTextureData();
        Array.Fill(raw, (byte)200);
        texture.LoadRawTextureData(raw);
        FontEngine.ResetAtlasTexture(texture);
        Expect.Equal(0, NonZero(texture), "ResetAtlasTexture clears the texture to transparent");
        Expect.Throws<ArgumentNullException>(() => FontEngine.ResetAtlasTexture(null!), "ResetAtlasTexture(null)");
    }

    private static void FaceEndsWithTheWorld()
    {
        FontEngine.LoadFontFace(SyntheticFont.Bytes(), 30);
        Expect.Equal(1u, FontEngine.GetGlyphIndex('A'), "the face is loaded");
        DisunityHost.UnloadWorld();
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "the face is gone with its world");
        DisunityHost.LoadWorld();
        Expect.Equal(0u, FontEngine.GetGlyphIndex('A'), "a new world has no face");
        Expect.Equal(FontEngineError.Invalid_Face, FontEngine.SetFaceSize(20), "and SetFaceSize reports it");
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
