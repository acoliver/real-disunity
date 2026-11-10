// The TextCore.LowLevel enums of the font engine, written from the engine's own knowledge of the font
// library's numbers and from what the engine, its tests and Old World name. Error codes and glyph load flags
// carry the FreeType library's own numbers. Only the members something names are declared.

namespace UnityEngine.TextCore.LowLevel
{
    public enum FontEngineError
    {
        Success = 0,
        Invalid_File_Path = 1,
        Invalid_File_Format = 2,
        Invalid_Pixel_Size = 23,
        Invalid_Library = 33,
        Invalid_Face = 35,
    }

    [System.Flags]
    public enum GlyphLoadFlags
    {
        LOAD_DEFAULT = 0,
        LOAD_NO_HINTING = 2,
        LOAD_RENDER = 4,
        LOAD_COLOR = 1048576,
    }

    public enum GlyphPackingMode
    {
        BestShortSideFit = 0,
        BestLongSideFit = 1,
        BestAreaFit = 2,
        BottomLeftRule = 3,
        ContactPointRule = 4,
    }

    /// <summary>How a glyph is rasterised. Each value combines raster mode bits; the scale of a distance field is bit 12
    /// (1x), 13 (8x), 14 (16x) or 15 (32x).</summary>
    public enum GlyphRenderMode
    {
        SMOOTH = 4117,
        RASTER_HINTED = 4122,
        COLOR = 33558549,
        COLOR_HINTED = 33558553,
        SDF = 4134,
        SDF8 = 8230,
        SDF16 = 16422,
        SDF32 = 32806,
        SDFAA = 4165,
        SDFAA_HINTED = 4169,
    }

    [System.Flags]
    public enum FontFeatureLookupFlags
    {
        None = 0,
        IgnoreLigatures = 4,
        IgnoreSpacingAdjustments = 256,
    }
}
