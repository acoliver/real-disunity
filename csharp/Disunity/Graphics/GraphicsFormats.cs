using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The conversions between Unity's three format vocabularies: <see cref="TextureFormat"/>,
/// <see cref="RenderTextureFormat"/> and <see cref="GraphicsFormat"/>. Only the formats listed in the
/// tables convert; any other format throws <see cref="NotSupportedException"/> naming it.
/// </summary>
internal static class GraphicsFormats
{
    /// <summary>The project's color space, which picks the sRGB or linear variant of a default format.
    /// The engine has no player settings yet and assumes Linear, as URP projects use.</summary>
    internal static ColorSpace ActiveColorSpace { get; set; } = ColorSpace.Linear;

    private readonly record struct RenderEntry(RenderTextureFormat Format, GraphicsFormat Linear, GraphicsFormat Srgb);

    private static RenderEntry Plain(RenderTextureFormat format, GraphicsFormat both) => new(format, both, both);

    private static readonly RenderEntry[] RenderTable =
    [
        new(RenderTextureFormat.ARGB32, GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormat.R8G8B8A8_SRGB),
        new(RenderTextureFormat.BGRA32, GraphicsFormat.B8G8R8A8_UNorm, GraphicsFormat.B8G8R8A8_SRGB),
        new(RenderTextureFormat.R8, GraphicsFormat.R8_UNorm, GraphicsFormat.R8_SRGB),
        new(RenderTextureFormat.RG16, GraphicsFormat.R8G8_UNorm, GraphicsFormat.R8G8_SRGB),
        Plain(RenderTextureFormat.ARGBHalf, GraphicsFormat.R16G16B16A16_SFloat),
        Plain(RenderTextureFormat.RGHalf, GraphicsFormat.R16G16_SFloat),
        Plain(RenderTextureFormat.RHalf, GraphicsFormat.R16_SFloat),
        Plain(RenderTextureFormat.ARGBFloat, GraphicsFormat.R32G32B32A32_SFloat),
        Plain(RenderTextureFormat.RGFloat, GraphicsFormat.R32G32_SFloat),
        Plain(RenderTextureFormat.RFloat, GraphicsFormat.R32_SFloat),
        Plain(RenderTextureFormat.R16, GraphicsFormat.R16_UNorm),
        Plain(RenderTextureFormat.RG32, GraphicsFormat.R16G16_UNorm),
        Plain(RenderTextureFormat.ARGB64, GraphicsFormat.R16G16B16A16_UNorm),
        Plain(RenderTextureFormat.RGB565, GraphicsFormat.B5G6R5_UNormPack16),
        Plain(RenderTextureFormat.ARGB4444, GraphicsFormat.B4G4R4A4_UNormPack16),
        Plain(RenderTextureFormat.ARGB2101010, GraphicsFormat.A2R10G10B10_UNormPack32),
        Plain(RenderTextureFormat.RGB111110Float, GraphicsFormat.B10G11R11_UFloatPack32),
        Plain(RenderTextureFormat.ARGBInt, GraphicsFormat.R32G32B32A32_SInt),
        Plain(RenderTextureFormat.RGInt, GraphicsFormat.R32G32_SInt),
        Plain(RenderTextureFormat.RInt, GraphicsFormat.R32_SInt),
    ];

    /// <summary>Whether the format stores sRGB-encoded color, which every <c>_SRGB</c> format does.</summary>
    internal static bool IsSrgb(GraphicsFormat format) => format.ToString().Contains("SRGB", StringComparison.Ordinal);

    /// <summary>The format with sRGB encoding on or off, or the format itself when it has no such pair.</summary>
    internal static GraphicsFormat WithSrgb(GraphicsFormat format, bool srgb)
    {
        foreach (var entry in RenderTable)
            if (entry.Linear == format || entry.Srgb == format) return srgb ? entry.Srgb : entry.Linear;
        foreach (var entry in TextureTable)
            if (entry.Linear == format || entry.Srgb == format) return srgb ? entry.Srgb : entry.Linear;
        return format;
    }

    /// <summary>The color format of a render texture. Depth and shadow map formats have no color.</summary>
    internal static GraphicsFormat ColorFormat(RenderTextureFormat format, bool srgb)
    {
        if (format == RenderTextureFormat.Default) format = RenderTextureFormat.ARGB32;
        if (format == RenderTextureFormat.DefaultHDR) format = RenderTextureFormat.ARGBHalf;
        if (format is RenderTextureFormat.Depth or RenderTextureFormat.Shadowmap) return GraphicsFormat.None;
        foreach (var entry in RenderTable)
            if (entry.Format == format) return srgb ? entry.Srgb : entry.Linear;
        throw new NotSupportedException($"RenderTextureFormat.{format} has no GraphicsFormat mapping yet");
    }

    /// <summary>The render texture format of a color format; the sRGB and linear variants share one.</summary>
    internal static RenderTextureFormat RenderFormat(GraphicsFormat format, bool shadow)
    {
        if (format == GraphicsFormat.None) return shadow ? RenderTextureFormat.Shadowmap : RenderTextureFormat.Depth;
        foreach (var entry in RenderTable)
            if (entry.Linear == format || entry.Srgb == format) return entry.Format;
        throw new NotSupportedException($"GraphicsFormat.{format} has no RenderTextureFormat mapping yet");
    }

    /// <summary>Whether a render texture stores sRGB color for a read/write choice; Default follows the project's color space.</summary>
    internal static bool UsesSrgb(RenderTextureReadWrite readWrite) => readWrite switch
    {
        RenderTextureReadWrite.sRGB => true,
        RenderTextureReadWrite.Linear => false,
        _ => ActiveColorSpace == ColorSpace.Linear,
    };

    /// <summary>The depth format for 0, 16, 24 or 32 depth buffer bits.</summary>
    internal static GraphicsFormat DepthFormat(int bits) => bits switch
    {
        0 => GraphicsFormat.None,
        16 => GraphicsFormat.D16_UNorm,
        24 => GraphicsFormat.D24_UNorm_S8_UInt,
        32 => GraphicsFormat.D32_SFloat,
        _ => throw new ArgumentException($"Unsupported depth buffer bits {bits}; use 0, 16, 24 or 32", nameof(bits)),
    };

    internal static int DepthBits(GraphicsFormat format) => format switch
    {
        GraphicsFormat.D16_UNorm or GraphicsFormat.D16_UNorm_S8_UInt => 16,
        GraphicsFormat.D24_UNorm or GraphicsFormat.D24_UNorm_S8_UInt => 24,
        GraphicsFormat.D32_SFloat or GraphicsFormat.D32_SFloat_S8_UInt => 32,
        _ => 0,
    };

    internal static bool HasStencil(GraphicsFormat format) => format is GraphicsFormat.D24_UNorm_S8_UInt
        or GraphicsFormat.D32_SFloat_S8_UInt or GraphicsFormat.D16_UNorm_S8_UInt or GraphicsFormat.S8_UInt;

    /// <summary>The color format a <see cref="DefaultFormat"/> picks for a render or script texture.</summary>
    internal static GraphicsFormat DefaultColor(DefaultFormat format) => format switch
    {
        DefaultFormat.LDR => ActiveColorSpace == ColorSpace.Linear ? GraphicsFormat.R8G8B8A8_SRGB : GraphicsFormat.R8G8B8A8_UNorm,
        DefaultFormat.HDR => GraphicsFormat.R16G16B16A16_SFloat,
        DefaultFormat.DepthStencil or DefaultFormat.Shadow => GraphicsFormat.None,
        _ => throw new NotSupportedException($"DefaultFormat.{format} has no GraphicsFormat mapping yet"),
    };

    private readonly record struct TextureEntry(TextureFormat Format, GraphicsFormat Linear, GraphicsFormat Srgb, bool Canonical);

    private static TextureEntry Tex(TextureFormat format, GraphicsFormat both, bool canonical = true) => new(format, both, both, canonical);

    private static TextureEntry Tex(TextureFormat format, GraphicsFormat linear, GraphicsFormat srgb, bool canonical = true) => new(format, linear, srgb, canonical);

    private static readonly TextureEntry[] TextureTable =
    [
        Tex(TextureFormat.R8, GraphicsFormat.R8_UNorm, GraphicsFormat.R8_SRGB),
        Tex(TextureFormat.Alpha8, GraphicsFormat.R8_UNorm, GraphicsFormat.R8_UNorm, canonical: false),
        Tex(TextureFormat.RG16, GraphicsFormat.R8G8_UNorm, GraphicsFormat.R8G8_SRGB),
        Tex(TextureFormat.RGB24, GraphicsFormat.R8G8B8_UNorm, GraphicsFormat.R8G8B8_SRGB),
        Tex(TextureFormat.RGBA32, GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormat.R8G8B8A8_SRGB),
        Tex(TextureFormat.ARGB32, GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormat.R8G8B8A8_SRGB, canonical: false),
        Tex(TextureFormat.BGRA32, GraphicsFormat.B8G8R8A8_UNorm, GraphicsFormat.B8G8R8A8_SRGB),
        Tex(TextureFormat.R16, GraphicsFormat.R16_UNorm),
        Tex(TextureFormat.RG32, GraphicsFormat.R16G16_UNorm),
        Tex(TextureFormat.RGB48, GraphicsFormat.R16G16B16_UNorm),
        Tex(TextureFormat.RGBA64, GraphicsFormat.R16G16B16A16_UNorm),
        Tex(TextureFormat.RHalf, GraphicsFormat.R16_SFloat),
        Tex(TextureFormat.RGHalf, GraphicsFormat.R16G16_SFloat),
        Tex(TextureFormat.RGBAHalf, GraphicsFormat.R16G16B16A16_SFloat),
        Tex(TextureFormat.RFloat, GraphicsFormat.R32_SFloat),
        Tex(TextureFormat.RGFloat, GraphicsFormat.R32G32_SFloat),
        Tex(TextureFormat.RGBAFloat, GraphicsFormat.R32G32B32A32_SFloat),
        Tex(TextureFormat.RGB565, GraphicsFormat.R5G6B5_UNormPack16),
        Tex(TextureFormat.RGBA4444, GraphicsFormat.R4G4B4A4_UNormPack16),
        Tex(TextureFormat.ARGB4444, GraphicsFormat.B4G4R4A4_UNormPack16),
        Tex(TextureFormat.RGB9e5Float, GraphicsFormat.E5B9G9R9_UFloatPack32),
        Tex(TextureFormat.R8_SIGNED, GraphicsFormat.R8_SNorm),
        Tex(TextureFormat.RG16_SIGNED, GraphicsFormat.R8G8_SNorm),
        Tex(TextureFormat.RGB24_SIGNED, GraphicsFormat.R8G8B8_SNorm),
        Tex(TextureFormat.RGBA32_SIGNED, GraphicsFormat.R8G8B8A8_SNorm),
        Tex(TextureFormat.R16_SIGNED, GraphicsFormat.R16_SNorm),
        Tex(TextureFormat.DXT1, GraphicsFormat.RGB_DXT1_UNorm, GraphicsFormat.RGB_DXT1_SRGB),
        Tex(TextureFormat.DXT5, GraphicsFormat.RGBA_DXT5_UNorm, GraphicsFormat.RGBA_DXT5_SRGB),
        Tex(TextureFormat.BC4, GraphicsFormat.R_BC4_UNorm),
        Tex(TextureFormat.BC5, GraphicsFormat.RG_BC5_UNorm),
        Tex(TextureFormat.BC6H, GraphicsFormat.RGB_BC6H_UFloat),
        Tex(TextureFormat.BC7, GraphicsFormat.RGBA_BC7_UNorm, GraphicsFormat.RGBA_BC7_SRGB),
        Tex(TextureFormat.ETC_RGB4, GraphicsFormat.RGB_ETC_UNorm),
        Tex(TextureFormat.ETC2_RGB, GraphicsFormat.RGB_ETC2_UNorm, GraphicsFormat.RGB_ETC2_SRGB),
        Tex(TextureFormat.ETC2_RGBA1, GraphicsFormat.RGB_A1_ETC2_UNorm, GraphicsFormat.RGB_A1_ETC2_SRGB),
        Tex(TextureFormat.ETC2_RGBA8, GraphicsFormat.RGBA_ETC2_UNorm, GraphicsFormat.RGBA_ETC2_SRGB),
        Tex(TextureFormat.EAC_R, GraphicsFormat.R_EAC_UNorm),
        Tex(TextureFormat.EAC_R_SIGNED, GraphicsFormat.R_EAC_SNorm),
        Tex(TextureFormat.EAC_RG, GraphicsFormat.RG_EAC_UNorm),
        Tex(TextureFormat.EAC_RG_SIGNED, GraphicsFormat.RG_EAC_SNorm),
        Tex(TextureFormat.ASTC_4x4, GraphicsFormat.RGBA_ASTC4X4_UNorm, GraphicsFormat.RGBA_ASTC4X4_SRGB),
        Tex(TextureFormat.ASTC_5x5, GraphicsFormat.RGBA_ASTC5X5_UNorm, GraphicsFormat.RGBA_ASTC5X5_SRGB),
        Tex(TextureFormat.ASTC_6x6, GraphicsFormat.RGBA_ASTC6X6_UNorm, GraphicsFormat.RGBA_ASTC6X6_SRGB),
        Tex(TextureFormat.ASTC_8x8, GraphicsFormat.RGBA_ASTC8X8_UNorm, GraphicsFormat.RGBA_ASTC8X8_SRGB),
        Tex(TextureFormat.ASTC_10x10, GraphicsFormat.RGBA_ASTC10X10_UNorm, GraphicsFormat.RGBA_ASTC10X10_SRGB),
        Tex(TextureFormat.ASTC_12x12, GraphicsFormat.RGBA_ASTC12X12_UNorm, GraphicsFormat.RGBA_ASTC12X12_SRGB),
        Tex(TextureFormat.ASTC_HDR_4x4, GraphicsFormat.RGBA_ASTC4X4_UFloat),
        Tex(TextureFormat.ASTC_HDR_5x5, GraphicsFormat.RGBA_ASTC5X5_UFloat),
        Tex(TextureFormat.ASTC_HDR_6x6, GraphicsFormat.RGBA_ASTC6X6_UFloat),
        Tex(TextureFormat.ASTC_HDR_8x8, GraphicsFormat.RGBA_ASTC8X8_UFloat),
        Tex(TextureFormat.ASTC_HDR_10x10, GraphicsFormat.RGBA_ASTC10X10_UFloat),
        Tex(TextureFormat.ASTC_HDR_12x12, GraphicsFormat.RGBA_ASTC12X12_UFloat),
        Tex(TextureFormat.PVRTC_RGB2, GraphicsFormat.RGB_PVRTC_2Bpp_UNorm, GraphicsFormat.RGB_PVRTC_2Bpp_SRGB),
        Tex(TextureFormat.PVRTC_RGBA2, GraphicsFormat.RGBA_PVRTC_2Bpp_UNorm, GraphicsFormat.RGBA_PVRTC_2Bpp_SRGB),
        Tex(TextureFormat.PVRTC_RGB4, GraphicsFormat.RGB_PVRTC_4Bpp_UNorm, GraphicsFormat.RGB_PVRTC_4Bpp_SRGB),
        Tex(TextureFormat.PVRTC_RGBA4, GraphicsFormat.RGBA_PVRTC_4Bpp_UNorm, GraphicsFormat.RGBA_PVRTC_4Bpp_SRGB),
    ];

    /// <summary>The graphics format of a script texture; 8-bit color formats pick sRGB unless <paramref name="linear"/>.</summary>
    internal static GraphicsFormat OfTexture(TextureFormat format, bool linear)
    {
        foreach (var entry in TextureTable)
            if (entry.Format == format) return linear ? entry.Linear : entry.Srgb;
        throw new NotSupportedException($"TextureFormat.{format} has no GraphicsFormat mapping yet");
    }

    /// <summary>The texture format that stores a graphics format.</summary>
    internal static TextureFormat ToTextureFormat(GraphicsFormat format)
    {
        foreach (var entry in TextureTable)
            if (entry.Canonical && (entry.Linear == format || entry.Srgb == format)) return entry.Format;
        throw new NotSupportedException($"GraphicsFormat.{format} has no TextureFormat mapping yet");
    }
}
