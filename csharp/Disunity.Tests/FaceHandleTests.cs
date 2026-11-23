using Disunity.Hosting;
using UnityEngine;
using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;

/// <summary>
/// A face the engine's own code loads (a font's glyph source) is separate from the one face TextMesh Pro has current in
/// FontEngine: each has its own size, its glyphs draw into whatever atlas the caller names, and loading or unloading one
/// leaves the other alone. CharacterInfo reads a glyph's metrics and atlas rectangle the way UI text expects them.
/// </summary>
internal static class FaceHandleTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            AHandleKeepsItsOwnSizeBesideTheCurrentFace();
            AHandleDrawsGlyphsIntoAnAtlas();
            AHandleEndsWithTheWorld();
        }
        finally { DisunityHost.UnloadWorld(); }
        CharacterInfoReadsMetricsAndRectangle();
        CharacterInfoOfAGlyphWithoutInk();
        Console.WriteLine("PASS: a face handle works beside FontEngine's current face and CharacterInfo reads glyph metrics and UVs");
    }

    private static FaceHandle Load(float size)
    {
        Expect.That(FaceHandle.TryLoad(SyntheticFont.Bytes(), size, 0, out var handle), "a handle loads from font bytes");
        return handle;
    }

    private static void AHandleKeepsItsOwnSizeBesideTheCurrentFace()
    {
        var handle = Load(20);
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(SyntheticFont.Bytes(), 10, 0), "the current face");
        Expect.That(handle.TryInfo(out var info), "info of the handle's face");
        Expect.Near(20, info.point_size, "the handle's size");
        Expect.Near(16, info.ascent_line, "the handle's ascent");
        Expect.Near(10, FontEngine.GetFaceInfo().pointSize, "the current face keeps its size");
        Expect.Equal(1u, handle.GlyphIndex('A'), "the handle maps a code point");
        Expect.Equal(0u, handle.GlyphIndex(0x4e2d), "the handle has no glyph for a code point the font lacks");
        var glyph = FontEngine.GlyphOf(handle, 1) ?? throw new InvalidOperationException("the handle has glyph 1");
        Expect.Near(12, glyph.metrics.horizontalAdvance, "advance at the handle's size");
        Expect.That(handle.SetSize(40), "the handle changes its size");
        Expect.Near(24, FontEngine.GlyphOf(handle, 1)!.metrics.horizontalAdvance, "advance at the new size");
        Expect.Near(10, FontEngine.GetFaceInfo().pointSize, "the current face is still 10 points");
        FontEngine.UnloadFontFace();
        Expect.That(handle.TryInfo(out _), "unloading the current face leaves the handle's face");
        handle.Unload();
        Expect.That(!handle.TryInfo(out _), "an unloaded handle has no face");
    }

    private static void AHandleDrawsGlyphsIntoAnAtlas()
    {
        var handle = Load(20);
        var texture = new Texture2D(64, 64, TextureFormat.Alpha8, false);
        var target = new FontEngine.AtlasTarget(1, GlyphPackingMode.BestShortSideFit, [new GlyphRect(0, 0, 64, 64)], [], GlyphRenderMode.SMOOTH, texture);
        var glyph = FontEngine.TryAdd(handle, 1, target) ?? throw new InvalidOperationException("the glyph fits");
        Expect.Equal(new GlyphRect(1, 1, 10, 14), glyph.glyphRect, "the glyph's rectangle without padding");
        Expect.That(texture.GetRawTextureData().Any(b => b > 200), "the glyph is drawn");
        Expect.Equal(0, FontEngine.GetFaceInfo().faceIndex, "FontEngine's current face is untouched by the draw");
        handle.Unload();
    }

    private static void AHandleEndsWithTheWorld()
    {
        var handle = Load(20);
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        Expect.That(!handle.TryInfo(out _), "a handle from an unloaded world has no face");
        Expect.That(FontEngine.GlyphOf(handle, 1) is null, "and no glyph");
        Expect.Equal(0u, handle.GlyphIndex('A'), "and no glyph index");
    }

    private static void CharacterInfoReadsMetricsAndRectangle()
    {
        var glyph = new Glyph(1, new GlyphMetrics(9.6f, 13.4f, 1.4f, 13.6f, 11.6f), new GlyphRect(4, 8, 10, 14));
        var info = CharacterInfo.Of('A', 20, FontStyle.Bold, glyph, 256, 128);
        Expect.That(info.index == 'A' && info.size == 20 && info.style == FontStyle.Bold, "identity");
        Expect.Equal(12, info.advance, "advance is rounded");
        Expect.That(info.glyphWidth == 10 && info.glyphHeight == 14, "glyph size is the rectangle's");
        Expect.That(info.minX == 1 && info.maxX == 11 && info.maxY == 14 && info.minY == 0 && info.bearing == 1, "box from rounded bearings");
        Expect.Equal(new Vector2(4f / 256, 8f / 128), info.uvBottomLeft, "uvBottomLeft");
        Expect.Equal(new Vector2(14f / 256, 8f / 128), info.uvBottomRight, "uvBottomRight");
        Expect.Equal(new Vector2(14f / 256, 22f / 128), info.uvTopRight, "uvTopRight");
        Expect.Equal(new Vector2(4f / 256, 22f / 128), info.uvTopLeft, "uvTopLeft");
    }

    private static void CharacterInfoOfAGlyphWithoutInk()
    {
        var info = CharacterInfo.Of(' ', 20, FontStyle.Normal, new Glyph(2, new GlyphMetrics(0, 0, 0, 0, 6), GlyphRect.zero), 256, 256);
        Expect.That(info.advance == 6 && info.glyphWidth == 0 && info.glyphHeight == 0, "a space advances and draws nothing");
        Expect.That(info.uvBottomLeft == info.uvTopRight && info.uvTopLeft == info.uvBottomRight, "its UVs collapse to one point");
    }
}
