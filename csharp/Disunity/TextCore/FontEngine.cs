namespace UnityEngine.TextCore.LowLevel;

/// <summary>A font face installed on the machine, found by family and style name.</summary>
[Serializable]
public struct FontReference
{
    public string familyName;
    public string styleName;
    public int faceIndex;
    public string filePath;
}

/// <summary>
/// The font library behind TextMesh Pro: it loads a font face, reads glyphs and kerning from it and
/// draws glyphs into an atlas texture. The engine's choice is that no font face is ever loaded,
/// because font assets that ship pre-baked atlases and glyph tables never need one. Every
/// <c>LoadFontFace</c> overload returns <see cref="FontEngineError.Invalid_File_Format"/>, the code
/// for a font file the library could not use, and <see cref="SetFaceSize"/> returns
/// <see cref="FontEngineError.Invalid_Face"/>. With no face, the queries answer as the library does
/// for an unloaded face: a default <see cref="FaceInfo"/>, glyph index 0, no glyph, no records and
/// no installed fonts, and nothing is added to an atlas. TextMesh Pro reaches some members through
/// <c>InternalsVisibleTo</c> where Unity declares them internal; the engine declares them public so
/// its module facade can forward the type. A member a player reports missing is added with the
/// signature the report gives.
/// </summary>
public static class FontEngine
{
    private const FontEngineError NoFace = FontEngineError.Invalid_Face;
    private const FontEngineError UnusableFile = FontEngineError.Invalid_File_Format;

    public static FontEngineError InitializeFontEngine() => FontEngineError.Success;

    public static FontEngineError DestroyFontEngine() => FontEngineError.Success;

    public static FontEngineError UnloadFontFace() => FontEngineError.Success;

    public static FontEngineError UnloadAllFontFaces() => FontEngineError.Success;

    public static FontEngineError LoadFontFace(string filePath) => UnusableFile;

    public static FontEngineError LoadFontFace(string filePath, int pointSize) => UnusableFile;

    public static FontEngineError LoadFontFace(string filePath, int pointSize, int faceIndex) => UnusableFile;

    public static FontEngineError LoadFontFace(string familyName, string styleName) => UnusableFile;

    public static FontEngineError LoadFontFace(string familyName, string styleName, float pointSize) => UnusableFile;

    public static FontEngineError LoadFontFace(byte[] sourceFontFile) => UnusableFile;

    public static FontEngineError LoadFontFace(byte[] sourceFontFile, int pointSize) => UnusableFile;

    public static FontEngineError LoadFontFace(byte[] sourceFontFile, int pointSize, int faceIndex) => UnusableFile;

    public static FontEngineError LoadFontFace(Font font) => UnusableFile;

    public static FontEngineError LoadFontFace(Font font, int pointSize) => UnusableFile;

    public static FontEngineError LoadFontFace(Font font, int pointSize, int faceIndex) => UnusableFile;

    public static FontEngineError SetFaceSize(int pointSize) => NoFace;

    public static FaceInfo GetFaceInfo() => default;

    /// <summary>
    /// Clears an atlas texture to transparent before glyphs are drawn into it. Nothing is ever drawn
    /// with no face, and a font asset asks for this only for an atlas that has no pixels, so the
    /// texture is left as it is, which is the engine's choice.
    /// </summary>
    public static void ResetAtlasTexture(Texture2D texture) => ArgumentNullException.ThrowIfNull(texture);

    public static string[] GetSystemFontNames() => [];

    public static FontReference[] GetSystemFontReferences() => [];

    public static bool TryGetSystemFontReference(string familyName, string styleName, out FontReference fontRef)
    {
        fontRef = default;
        return false;
    }

    public static uint GetGlyphIndex(uint unicode) => 0;

    public static bool TryGetGlyphIndex(uint unicode, out uint glyphIndex)
    {
        glyphIndex = 0;
        return false;
    }

    public static bool TryGetGlyphWithUnicodeValue(uint unicode, GlyphLoadFlags flags, out Glyph? glyph)
    {
        glyph = null;
        return false;
    }

    public static bool TryGetGlyphWithIndexValue(uint glyphIndex, GlyphLoadFlags flags, out Glyph? glyph)
    {
        glyph = null;
        return false;
    }

    public static bool TryAddGlyphToTexture(uint glyphIndex, int padding, GlyphPackingMode packingMode,
        List<GlyphRect> freeGlyphRects, List<GlyphRect> usedGlyphRects, GlyphRenderMode renderMode, Texture2D texture, out Glyph? glyph)
    {
        glyph = null;
        return false;
    }

    public static bool TryAddGlyphsToTexture(List<uint> glyphIndexes, int padding, GlyphPackingMode packingMode,
        List<GlyphRect> freeGlyphRects, List<GlyphRect> usedGlyphRects, GlyphRenderMode renderMode, Texture2D texture, out Glyph[] glyphs)
    {
        glyphs = [];
        return false;
    }

    public static bool TryAddGlyphsToTexture(List<Glyph> glyphsToAdd, List<Glyph> glyphsAdded, int padding, GlyphPackingMode packingMode,
        List<GlyphRect> freeGlyphRects, List<GlyphRect> usedGlyphRects, GlyphRenderMode renderMode, Texture2D texture)
    {
        glyphsAdded.Clear();
        return false;
    }

    public static GlyphPairAdjustmentRecord[] GetGlyphPairAdjustmentTable(uint[] glyphIndexes) => [];

    public static MarkToBaseAdjustmentRecord[] GetMarkToBaseAdjustmentRecords(uint glyphIndex) => [];

    public static MarkToMarkAdjustmentRecord[] GetMarkToMarkAdjustmentRecords(uint glyphIndex) => [];

    public static LigatureSubstitutionRecord[] GetLigatureSubstitutionRecords(uint glyphIndex) => [];
}
