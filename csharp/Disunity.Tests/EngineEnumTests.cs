using UnityEngine;
using UnityEngine.Assertions;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using UnityEngine.SceneManagement;
using UnityEngine.TextCore.LowLevel;

/// <summary>
/// Spot checks of the engine's enums against the numbers a player and the Rust side use, and the message of a
/// failed assertion.
/// </summary>
internal static class EngineEnumTests
{
    public static void Run()
    {
        Require((int)TextureFormat.RGBA32 == 4 && (int)TextureFormat.ARGB32 == 5, "TextureFormat");
        Require((int)RenderTextureFormat.ARGB32 == 0 && (int)RenderTextureFormat.Depth == 1, "RenderTextureFormat");
        Require((DepthTextureMode.Depth | DepthTextureMode.MotionVectors) == (DepthTextureMode)5
            && typeof(DepthTextureMode).IsDefined(typeof(FlagsAttribute), false), "DepthTextureMode is a flags enum");
        Require((int)HideFlags.DontSave == 52 && (int)HideFlags.HideAndDontSave == 61, "HideFlags");
        Require((int)CameraType.Game == 1 && (int)CameraType.SceneView == 2, "CameraType");
        Require((int)FilterMode.Point == 0 && (int)FilterMode.Trilinear == 2, "FilterMode");
        Require((int)BlendMode.SrcAlpha == 5 && (int)BlendMode.OneMinusSrcAlpha == 10, "BlendMode");
        Require((int)CompareFunction.LessEqual == 4 && (int)CompareFunction.Always == 8, "CompareFunction");
        Require((int)CullMode.Back == 2, "CullMode");
        Require((int)GraphicsDeviceType.Direct3D11 == 2 && (int)GraphicsDeviceType.Vulkan == 21, "GraphicsDeviceType");
        Require((int)LoadSceneMode.Single == 0 && (int)LoadSceneMode.Additive == 1, "LoadSceneMode");
        RequireGraphicsFormats();
        RequireFormatUsage();
        RequireFontEnums();
        Require(new AssertionException("Value was null.", "camera").Message == "Value was null.\ncamera"
            && new AssertionException("Value was null.", null).Message == "Value was null.", "an assertion's message adds the caller's");
        Console.WriteLine("PASS: engine enums match the numbers players and the host use");
    }

    /// <summary>The GraphicsFormat numbers the host maps to GPU formats (render textures cross the ABI as these
    /// integers), the depth formats Old World's render textures use, and the families around them.</summary>
    private static void RequireGraphicsFormats()
    {
        (GraphicsFormat Format, int Value)[] pinned =
        [
            (GraphicsFormat.None, 0), (GraphicsFormat.R8G8B8A8_SRGB, 4), (GraphicsFormat.R8_UNorm, 5),
            (GraphicsFormat.R8G8B8A8_UNorm, 8), (GraphicsFormat.R8G8B8A8_SNorm, 12), (GraphicsFormat.R8G8B8A8_UInt, 16),
            (GraphicsFormat.R16_UNorm, 21), (GraphicsFormat.R16G16B16A16_UNorm, 24), (GraphicsFormat.R32_UInt, 37),
            (GraphicsFormat.R32G32B32A32_SInt, 44), (GraphicsFormat.R16_SFloat, 45), (GraphicsFormat.R16G16_SFloat, 46),
            (GraphicsFormat.R16G16B16A16_SFloat, 48), (GraphicsFormat.R32_SFloat, 49), (GraphicsFormat.R32G32B32A32_SFloat, 52),
            (GraphicsFormat.B8G8R8A8_SRGB, 57), (GraphicsFormat.B8G8R8A8_UNorm, 59), (GraphicsFormat.R5G6B5_UNormPack16, 68),
            (GraphicsFormat.B10G11R11_UFloatPack32, 74), (GraphicsFormat.A2B10G10R10_UNormPack32, 75),
            (GraphicsFormat.D16_UNorm, 90), (GraphicsFormat.D24_UNorm, 91), (GraphicsFormat.D24_UNorm_S8_UInt, 92),
            (GraphicsFormat.D32_SFloat, 93), (GraphicsFormat.D32_SFloat_S8_UInt, 94), (GraphicsFormat.S8_UInt, 95),
            (GraphicsFormat.RGBA_DXT1_SRGB, 96), (GraphicsFormat.RGBA_DXT1_UNorm, 97), (GraphicsFormat.RGBA_DXT5_SRGB, 100),
            (GraphicsFormat.RGBA_DXT5_UNorm, 101), (GraphicsFormat.R_BC4_UNorm, 102), (GraphicsFormat.RGB_BC6H_UFloat, 106),
            (GraphicsFormat.RGBA_BC7_SRGB, 108), (GraphicsFormat.RGBA_BC7_UNorm, 109),
        ];
        foreach (var (format, value) in pinned) Require((int)format == value, $"GraphicsFormat.{format} is {value}");
    }

    /// <summary>FormatUsage counts bits of GraphicsFormatUsage, and the flags are a set.</summary>
    private static void RequireFormatUsage()
    {
        Require((int)GraphicsFormatUsage.Sample == 1 << (int)FormatUsage.Sample
            && (int)GraphicsFormatUsage.Render == 1 << (int)FormatUsage.Render
            && (int)GraphicsFormatUsage.Blend == 1 << (int)FormatUsage.Blend, "FormatUsage indexes the bits of GraphicsFormatUsage");
        Require(typeof(GraphicsFormatUsage).IsDefined(typeof(FlagsAttribute), false)
            && typeof(TextureCreationFlags).IsDefined(typeof(FlagsAttribute), false)
            && (int)TextureCreationFlags.MipChain == 1, "TextureCreationFlags and GraphicsFormatUsage are flag sets");
    }

    /// <summary>The font library's error codes and glyph load flags, and the render mode Old World's font assets carry.</summary>
    private static void RequireFontEnums()
    {
        Require((int)FontEngineError.Success == 0 && (int)FontEngineError.Invalid_File_Path == 1
            && (int)FontEngineError.Invalid_Pixel_Size == 23 && (int)FontEngineError.Invalid_Library == 33
            && (int)FontEngineError.Invalid_Face == 35, "FontEngineError");
        Require((int)GlyphLoadFlags.LOAD_DEFAULT == 0 && (int)GlyphLoadFlags.LOAD_NO_HINTING == 2
            && (int)GlyphLoadFlags.LOAD_RENDER == 4 && (int)GlyphLoadFlags.LOAD_COLOR == 1 << 20, "GlyphLoadFlags");
        Require((int)GlyphRenderMode.SDFAA == 4165, "GlyphRenderMode.SDFAA");
        Require((int)GlyphPackingMode.BestShortSideFit == 0 && (int)GlyphPackingMode.ContactPointRule == 4, "GlyphPackingMode");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"engine enums: {message}");
    }
}
