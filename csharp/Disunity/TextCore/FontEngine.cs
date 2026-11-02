using Disunity.Hosting;
using Disunity.Native;

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
/// The font library behind TextMesh Pro: it loads one font face at a time, reads glyphs and kerning from it and
/// draws glyphs into an atlas texture. The face, its measurements and the glyph drawing live in the world
/// simulation (see <c>disunity_face_load</c>); this class validates arguments, moves bytes and records, and packs
/// and writes the atlas. Loading a face replaces the current one. A face belongs to the loaded world and is gone
/// when the world is. The family and system font overloads of <c>LoadFontFace</c> return
/// <see cref="FontEngineError.Invalid_File_Format"/> because the engine has no system fonts, a font with no file
/// data is unusable the same way, and with no face the queries answer as for an unloaded face: a default
/// <see cref="FaceInfo"/>, glyph index 0, no glyph, no records and no installed fonts. A face loaded without a
/// point size starts at 16 points. TextMesh Pro reaches some members through <c>InternalsVisibleTo</c> where Unity
/// declares them internal; the engine declares them public so its module facade can forward the type. A member a
/// player reports missing is added with the signature the report gives.
/// </summary>
public static unsafe partial class FontEngine
{
    private const FontEngineError NoFace = FontEngineError.Invalid_Face;
    private const FontEngineError UnusableFile = FontEngineError.Invalid_File_Format;
    private const float DefaultPointSize = 16;

    private static bool _hasFace;
    private static ulong _face;
    private static ulong _faceWorld;

    /// <summary>Whether a face is loaded in the loaded world.</summary>
    private static bool HasFace => _hasFace && DisunityHost.IsLoaded && _faceWorld == DisunityHost.WorldHandle;

    public static FontEngineError InitializeFontEngine() => FontEngineError.Success;

    public static FontEngineError DestroyFontEngine() => UnloadFontFace();

    public static FontEngineError UnloadFontFace()
    {
        if (HasFace) NativeBindings.disunity_face_unload(_faceWorld, _face);
        _hasFace = false;
        return FontEngineError.Success;
    }

    public static FontEngineError UnloadAllFontFaces() => UnloadFontFace();

    public static FontEngineError LoadFontFace(string filePath) => LoadFontFace(filePath, DefaultPointSize, 0);

    public static FontEngineError LoadFontFace(string filePath, float pointSize) => LoadFontFace(filePath, pointSize, 0);

    public static FontEngineError LoadFontFace(string filePath, float pointSize, int faceIndex)
    {
        byte[] data;
        try { data = File.ReadAllBytes(filePath); }
        catch (Exception error) when (error is IOException or UnauthorizedAccessException or ArgumentException or NotSupportedException)
        {
            UnloadFontFace();
            return FontEngineError.Invalid_File_Path;
        }
        return LoadFontFace(data, pointSize, faceIndex);
    }

    public static FontEngineError LoadFontFace(string familyName, string styleName) => Unusable();

    public static FontEngineError LoadFontFace(string familyName, string styleName, float pointSize) => Unusable();

    public static FontEngineError LoadFontFace(byte[] sourceFontFile) => LoadFontFace(sourceFontFile, DefaultPointSize, 0);

    public static FontEngineError LoadFontFace(byte[] sourceFontFile, float pointSize) => LoadFontFace(sourceFontFile, pointSize, 0);

    public static FontEngineError LoadFontFace(byte[] sourceFontFile, float pointSize, int faceIndex)
    {
        UnloadFontFace();
        if (sourceFontFile is null || sourceFontFile.Length == 0 || faceIndex < 0) return UnusableFile;
        if (!(pointSize > 0) || !float.IsFinite(pointSize)) return FontEngineError.Invalid_Pixel_Size;
        if (!DisunityHost.IsLoaded) return FontEngineError.Invalid_Library;
        ulong face;
        DisunityResult result;
        fixed (byte* bytes = sourceFontFile)
            result = NativeBindings.disunity_face_load(DisunityHost.WorldHandle, bytes, (uint)sourceFontFile.Length, (uint)faceIndex, pointSize, &face);
        if (result.code != 0) return UnusableFile;
        (_hasFace, _face, _faceWorld) = (true, face, DisunityHost.WorldHandle);
        return FontEngineError.Success;
    }

    public static FontEngineError LoadFontFace(Font font) => LoadFontFace(font, DefaultPointSize, 0);

    public static FontEngineError LoadFontFace(Font font, float pointSize) => LoadFontFace(font, pointSize, 0);

    public static FontEngineError LoadFontFace(Font font, float pointSize, int faceIndex)
    {
        ArgumentNullException.ThrowIfNull(font);
        return font.FontData is { } data ? LoadFontFace(data, pointSize, faceIndex) : Unusable();
    }

    private static FontEngineError Unusable()
    {
        UnloadFontFace();
        return UnusableFile;
    }

    public static FontEngineError SetFaceSize(int pointSize)
    {
        if (!HasFace) return NoFace;
        return NativeBindings.disunity_face_set_size(_faceWorld, _face, pointSize).code == 0 ? FontEngineError.Success : FontEngineError.Invalid_Pixel_Size;
    }

    public static FaceInfo GetFaceInfo()
    {
        if (!HasFace) return default;
        DisunityFaceInfo native;
        if (NativeBindings.disunity_face_info(_faceWorld, _face, &native).code != 0) return default;
        return new FaceInfo
        {
            faceIndex = (int)native.face_index, familyName = FaceName(0), styleName = FaceName(1), pointSize = native.point_size,
            scale = native.scale, unitsPerEM = (int)native.units_per_em, lineHeight = native.line_height, ascentLine = native.ascent_line,
            capLine = native.cap_line, meanLine = native.mean_line, baseline = native.baseline, descentLine = native.descent_line,
            superscriptOffset = native.superscript_offset, superscriptSize = native.superscript_size,
            subscriptOffset = native.subscript_offset, subscriptSize = native.subscript_size,
            underlineOffset = native.underline_offset, underlineThickness = native.underline_thickness,
            strikethroughOffset = native.strikethrough_offset, strikethroughThickness = native.strikethrough_thickness, tabWidth = native.tab_width,
        };
    }

    private static string FaceName(uint which)
    {
        uint length;
        if (NativeBindings.disunity_face_name(_faceWorld, _face, which, null, 0, &length).code != 0 || length == 0) return string.Empty;
        var bytes = new byte[length];
        fixed (byte* pointer = bytes)
            if (NativeBindings.disunity_face_name(_faceWorld, _face, which, pointer, length, &length).code != 0) return string.Empty;
        return System.Text.Encoding.UTF8.GetString(bytes);
    }

    public static string[] GetSystemFontNames() => [];

    public static FontReference[] GetSystemFontReferences() => [];

    public static bool TryGetSystemFontReference(string familyName, string styleName, out FontReference fontRef)
    {
        fontRef = default;
        return false;
    }

    public static uint GetGlyphIndex(uint unicode)
    {
        uint glyph = 0;
        if (HasFace) NativeBindings.disunity_face_glyph_index(_faceWorld, _face, unicode, &glyph);
        return glyph;
    }

    public static bool TryGetGlyphIndex(uint unicode, out uint glyphIndex)
    {
        glyphIndex = GetGlyphIndex(unicode);
        return glyphIndex != 0;
    }

    public static bool TryGetGlyphWithUnicodeValue(uint unicode, GlyphLoadFlags flags, out Glyph? glyph)
    {
        var index = GetGlyphIndex(unicode);
        glyph = index == 0 ? null : GlyphOf(index);
        return glyph != null;
    }

    public static bool TryGetGlyphWithIndexValue(uint glyphIndex, GlyphLoadFlags flags, out Glyph? glyph)
    {
        glyph = GlyphOf(glyphIndex);
        return glyph != null;
    }

    /// <summary>A glyph with its metrics at the face's size and no place in an atlas, or null with no face or no such glyph.</summary>
    private static Glyph? GlyphOf(uint index)
    {
        if (!HasFace) return null;
        DisunityGlyphMetrics native;
        if (NativeBindings.disunity_face_glyph_metrics(_faceWorld, _face, index, &native).code != 0) return null;
        var metrics = new GlyphMetrics(native.width, native.height, native.bearing_x, native.bearing_y, native.advance);
        return new Glyph(index, metrics, GlyphRect.zero, 1, 0);
    }

    /// <summary>Kerning of every ordered pair of the glyphs the font's <c>kern</c> table adjusts. The <c>GPOS</c> pair
    /// tables are not read, which is the engine's choice, so a font that kerns only through them has no records.</summary>
    public static GlyphPairAdjustmentRecord[] GetGlyphPairAdjustmentTable(uint[] glyphIndexes)
    {
        ArgumentNullException.ThrowIfNull(glyphIndexes);
        if (!HasFace || glyphIndexes.Length == 0) return [];
        uint count = 0;
        fixed (uint* glyphs = glyphIndexes)
        {
            if (NativeBindings.disunity_face_pairs(_faceWorld, _face, glyphs, (uint)glyphIndexes.Length, null, &count).code != 0 || count == 0) return [];
            var native = new DisunityGlyphPair[count];
            fixed (DisunityGlyphPair* records = native)
                if (NativeBindings.disunity_face_pairs(_faceWorld, _face, glyphs, (uint)glyphIndexes.Length, records, &count).code != 0) return [];
            return Array.ConvertAll(native, pair => new GlyphPairAdjustmentRecord(
                new GlyphAdjustmentRecord(pair.first, new GlyphValueRecord(0, 0, pair.x_advance, 0)),
                new GlyphAdjustmentRecord(pair.second, default)));
        }
    }

    public static MarkToBaseAdjustmentRecord[] GetMarkToBaseAdjustmentRecords(uint glyphIndex) => [];

    public static MarkToMarkAdjustmentRecord[] GetMarkToMarkAdjustmentRecords(uint glyphIndex) => [];

    public static LigatureSubstitutionRecord[] GetLigatureSubstitutionRecords(uint glyphIndex) => [];
}
