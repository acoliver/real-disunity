using UnityEngine;
using UnityEngine.Experimental.Rendering;

/// <summary>GraphicsFormatUtility converts between the three format vocabularies and reads depth and sRGB facts.</summary>
internal static class GraphicsFormatUtilityTests
{
    public static void Run()
    {
        Conversions();
        Facts();
        BlockSizes();
        Unsupported();
        Console.WriteLine("PASS: GraphicsFormatUtility converts texture, render texture and graphics formats and reports depth, stencil and sRGB facts");
    }

    private static void Conversions()
    {
        Expect.Equal(GraphicsFormat.R8G8B8A8_SRGB, GraphicsFormatUtility.GetGraphicsFormat(TextureFormat.RGBA32, true), "RGBA32 sRGB");
        Expect.Equal(GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormatUtility.GetGraphicsFormat(TextureFormat.RGBA32, false), "RGBA32 linear");
        Expect.Equal(GraphicsFormat.R16G16B16A16_SFloat, GraphicsFormatUtility.GetGraphicsFormat(RenderTextureFormat.ARGBHalf, true), "ARGBHalf has one variant");
        Expect.Equal(GraphicsFormat.B8G8R8A8_SRGB, GraphicsFormatUtility.GetGraphicsFormat(RenderTextureFormat.BGRA32, true), "BGRA32 sRGB");
        Expect.Equal(GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormatUtility.GetGraphicsFormat(RenderTextureFormat.ARGB32, false), "ARGB32 linear");
        Expect.Equal(GraphicsFormat.R8G8B8A8_SRGB, GraphicsFormatUtility.GetGraphicsFormat(RenderTextureFormat.ARGB32, RenderTextureReadWrite.sRGB), "read/write sRGB");
        Expect.Equal(GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormatUtility.GetGraphicsFormat(RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear), "read/write linear");
        Expect.Equal(TextureFormat.RGBA32, GraphicsFormatUtility.GetTextureFormat(GraphicsFormat.R8G8B8A8_SRGB), "GetTextureFormat");
        Expect.Equal(RenderTextureFormat.ARGB32, GraphicsFormatUtility.GetRenderTextureFormat(GraphicsFormat.R8G8B8A8_SRGB), "GetRenderTextureFormat");
        Expect.Equal(GraphicsFormat.R8G8B8A8_SRGB, GraphicsFormatUtility.GetSRGBFormat(GraphicsFormat.R8G8B8A8_UNorm), "GetSRGBFormat");
        Expect.Equal(GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormatUtility.GetLinearFormat(GraphicsFormat.R8G8B8A8_SRGB), "GetLinearFormat");
        Expect.Equal(GraphicsFormat.R16_SFloat, GraphicsFormatUtility.GetSRGBFormat(GraphicsFormat.R16_SFloat), "no sRGB pair");
    }

    private static void Facts()
    {
        Expect.That(GraphicsFormatUtility.IsSRGBFormat(GraphicsFormat.R8G8B8A8_SRGB), "sRGB format");
        Expect.That(!GraphicsFormatUtility.IsSRGBFormat(GraphicsFormat.R8G8B8A8_UNorm), "linear format");
        Expect.That(GraphicsFormatUtility.IsDepthFormat(GraphicsFormat.D32_SFloat), "D32 is depth");
        Expect.That(!GraphicsFormatUtility.IsDepthFormat(GraphicsFormat.R8_UNorm), "R8 is not depth");
        Expect.That(GraphicsFormatUtility.IsStencilFormat(GraphicsFormat.D24_UNorm_S8_UInt), "D24S8 has stencil");
        Expect.That(!GraphicsFormatUtility.IsStencilFormat(GraphicsFormat.D32_SFloat), "D32 has no stencil");
        Expect.Equal(24u, GraphicsFormatUtility.GetDepthBits(GraphicsFormat.D24_UNorm_S8_UInt), "depth bits");
        Expect.Equal(8u, GraphicsFormatUtility.GetStencilBits(GraphicsFormat.D24_UNorm_S8_UInt), "stencil bits");
        Expect.Equal(0u, GraphicsFormatUtility.GetStencilBits(GraphicsFormat.D16_UNorm), "no stencil bits");
        Expect.Equal(GraphicsFormat.D16_UNorm, GraphicsFormatUtility.GetDepthStencilFormat(16), "16 depth bits");
        Expect.Equal(GraphicsFormat.None, GraphicsFormatUtility.GetDepthStencilFormat(0), "no depth bits");
        Expect.Equal("R8G8B8A8_SRGB", GraphicsFormatUtility.GetFormatString(GraphicsFormat.R8G8B8A8_SRGB), "GetFormatString");
        Expect.Equal("None", GraphicsFormatUtility.GetFormatString(GraphicsFormat.None), "GetFormatString of None");
        Expect.That(GraphicsFormatUtility.IsCompressedFormat(TextureFormat.DXT5), "DXT5 compressed");
        Expect.That(!GraphicsFormatUtility.IsCompressedFormat(TextureFormat.RGBA32), "RGBA32 not compressed");
    }

    private static void BlockSizes()
    {
        var sizes = new (GraphicsFormat Format, uint Bytes)[]
        {
            (GraphicsFormat.None, 0), (GraphicsFormat.R8_UNorm, 1), (GraphicsFormat.R8G8B8A8_SRGB, 4), (GraphicsFormat.R16G16B16A16_SFloat, 8),
            (GraphicsFormat.R32G32B32A32_SFloat, 16), (GraphicsFormat.R5G6B5_UNormPack16, 2), (GraphicsFormat.A2R10G10B10_UNormPack32, 4),
            (GraphicsFormat.E5B9G9R9_UFloatPack32, 4), (GraphicsFormat.D16_UNorm, 2), (GraphicsFormat.D32_SFloat, 4),
            (GraphicsFormat.D24_UNorm_S8_UInt, 4), (GraphicsFormat.S8_UInt, 1), (GraphicsFormat.RGB_DXT1_UNorm, 8),
            (GraphicsFormat.RGBA_DXT5_UNorm, 16), (GraphicsFormat.R_BC4_UNorm, 8), (GraphicsFormat.RG_BC5_UNorm, 16),
            (GraphicsFormat.RGBA_BC7_UNorm, 16), (GraphicsFormat.RGB_ETC2_UNorm, 8), (GraphicsFormat.RGBA_ETC2_UNorm, 16),
            (GraphicsFormat.R_EAC_UNorm, 8), (GraphicsFormat.RG_EAC_UNorm, 16), (GraphicsFormat.RGBA_ASTC6X6_UNorm, 16),
            (GraphicsFormat.RGB_PVRTC_2Bpp_UNorm, 8),
        };
        foreach (var (format, bytes) in sizes)
            Expect.Equal(bytes, GraphicsFormatUtility.GetBlockSize(format), $"GetBlockSize({format})");
        Expect.Equal(1u, GraphicsFormatUtility.GetBlockWidth(GraphicsFormat.R8G8B8A8_UNorm), "plain block width");
        Expect.Equal(1u, GraphicsFormatUtility.GetBlockHeight(GraphicsFormat.R8G8B8A8_UNorm), "plain block height");
        Expect.Equal(4u, GraphicsFormatUtility.GetBlockWidth(GraphicsFormat.RGBA_BC7_UNorm), "BC7 block width");
        Expect.Equal(4u, GraphicsFormatUtility.GetBlockHeight(GraphicsFormat.RGB_DXT1_UNorm), "DXT1 block height");
        Expect.Equal(6u, GraphicsFormatUtility.GetBlockWidth(GraphicsFormat.RGBA_ASTC6X6_UNorm), "ASTC 6x6 block width");
        Expect.Equal(6u, GraphicsFormatUtility.GetBlockHeight(GraphicsFormat.RGBA_ASTC6X6_UNorm), "ASTC 6x6 block height");
        Expect.Equal(8u, GraphicsFormatUtility.GetBlockWidth(GraphicsFormat.RGB_PVRTC_2Bpp_UNorm), "PVRTC 2bpp block width");
        Expect.Equal(4u, GraphicsFormatUtility.GetBlockHeight(GraphicsFormat.RGB_PVRTC_2Bpp_UNorm), "PVRTC 2bpp block height");
    }

    private static void Unsupported()
    {
        var error = Expect.Catch<NotSupportedException>(() => GraphicsFormatUtility.GetTextureFormat(GraphicsFormat.R8G8B8A8_SInt), "no mapping");
        Expect.That(error.Message.Contains("R8G8B8A8_SInt"), error.Message);
        Expect.Throws<ArgumentException>(() => GraphicsFormatUtility.GetDepthStencilFormat(20), "odd depth bits");
    }
}
