// Unity enums the player's scripts and packages use, generated from a Unity 6000.3 player's
// engine module metadata with their names and values. The engine reads few of them yet.

namespace JetBrains.Annotations
{
    [System.Flags]
    public enum CollectionAccessType
    {
        None = 0, Read = 1, ModifyExistingContent = 2, UpdatedContent = 6,
    }
}

namespace Unity.IO.LowLevel.Unsafe
{
    public enum FileState
    {
        Absent = 0, Exists = 1,
    }

    public enum ReadStatus
    {
        Complete = 0, InProgress = 1, Failed = 2, Truncated = 4, Canceled = 5,
    }
}

namespace Unity.Profiling
{
    public enum ProfilerMarkerDataUnit : byte
    {
        Undefined = 0, TimeNanoseconds = 1, Bytes = 2, Count = 3, Percent = 4, FrequencyHz = 5,
    }

    [System.Flags]
    public enum ProfilerCounterOptions : ushort
    {
        None = 0, FlushOnEndOfFrame = 2, ResetToZeroOnFlush = 4,
    }
}

namespace UnityEngine.Analytics
{
    public enum AnalyticsResult
    {
        Ok = 0, NotInitialized = 1, AnalyticsDisabled = 2, TooManyItems = 3, SizeLimitReached = 4,
        TooManyRequests = 5, InvalidData = 6, UnsupportedPlatform = 7,
    }
}

namespace UnityEngine.Diagnostics
{
    public enum ForcedCrashCategory
    {
        AccessViolation = 0, FatalError = 1, Abort = 2, PureVirtualFunction = 3, MonoAbort = 4,
    }
}

namespace UnityEngine.Experimental.GlobalIllumination
{
    public enum FalloffType : byte
    {
        InverseSquared = 0, InverseSquaredNoRangeAttenuation = 1, Linear = 2, Legacy = 3, Undefined = 4,
    }

    public enum AngularFalloffType : byte
    {
        LUT = 0, AnalyticAndInnerAngle = 1,
    }
}

namespace UnityEngine.Experimental.Rendering
{
    [System.Flags]
    public enum TextureCreationFlags
    {
        None = 0, MipChain = 1, DontInitializePixels = 4, Crunch = 64, DontUploadUponCreate = 1024,
        IgnoreMipmapLimit = 2048,
    }

    public enum FormatUsage
    {
        Sample = 0, Linear = 1, Sparse = 2, Render = 4, Blend = 5, GetPixels = 6, SetPixels = 7, SetPixels32
        = 8, ReadPixels = 9, LoadStore = 10, MSAA2x = 11, MSAA4x = 12, MSAA8x = 13, StencilSampling = 16,
    }

    [System.Flags]
    public enum GraphicsFormatUsage
    {
        None = 0, Sample = 1, Linear = 2, Sparse = 4, Render = 16, Blend = 32, GetPixels = 64, SetPixels =
        128, SetPixels32 = 256, ReadPixels = 512, LoadStore = 1024, MSAA2x = 2048, MSAA4x = 4096, MSAA8x =
        8192, StencilSampling = 65536,
    }

    public enum DefaultFormat
    {
        LDR = 0, HDR = 1, DepthStencil = 2, Shadow = 3, Video = 4,
    }

    public enum GraphicsFormat
    {
        None = 0, R8_SRGB = 1, R8G8_SRGB = 2, R8G8B8_SRGB = 3, R8G8B8A8_SRGB = 4, R8_UNorm = 5, R8G8_UNorm =
        6, R8G8B8_UNorm = 7, R8G8B8A8_UNorm = 8, R8_SNorm = 9, R8G8_SNorm = 10, R8G8B8_SNorm = 11,
        R8G8B8A8_SNorm = 12, R8_UInt = 13, R8G8_UInt = 14, R8G8B8_UInt = 15, R8G8B8A8_UInt = 16, R8_SInt =
        17, R8G8_SInt = 18, R8G8B8_SInt = 19, R8G8B8A8_SInt = 20, R16_UNorm = 21, R16G16_UNorm = 22,
        R16G16B16_UNorm = 23, R16G16B16A16_UNorm = 24, R16_SNorm = 25, R16G16_SNorm = 26, R16G16B16_SNorm =
        27, R16G16B16A16_SNorm = 28, R16_UInt = 29, R16G16_UInt = 30, R16G16B16_UInt = 31, R16G16B16A16_UInt
        = 32, R16_SInt = 33, R16G16_SInt = 34, R16G16B16_SInt = 35, R16G16B16A16_SInt = 36, R32_UInt = 37,
        R32G32_UInt = 38, R32G32B32_UInt = 39, R32G32B32A32_UInt = 40, R32_SInt = 41, R32G32_SInt = 42,
        R32G32B32_SInt = 43, R32G32B32A32_SInt = 44, R16_SFloat = 45, R16G16_SFloat = 46, R16G16B16_SFloat =
        47, R16G16B16A16_SFloat = 48, R32_SFloat = 49, R32G32_SFloat = 50, R32G32B32_SFloat = 51,
        R32G32B32A32_SFloat = 52, B8G8R8_SRGB = 56, B8G8R8A8_SRGB = 57, B8G8R8_UNorm = 58, B8G8R8A8_UNorm =
        59, B8G8R8_SNorm = 60, B8G8R8A8_SNorm = 61, B8G8R8_UInt = 62, B8G8R8A8_UInt = 63, B8G8R8_SInt = 64,
        B8G8R8A8_SInt = 65, R4G4B4A4_UNormPack16 = 66, B4G4R4A4_UNormPack16 = 67, R5G6B5_UNormPack16 = 68,
        B5G6R5_UNormPack16 = 69, R5G5B5A1_UNormPack16 = 70, B5G5R5A1_UNormPack16 = 71, A1R5G5B5_UNormPack16
        = 72, E5B9G9R9_UFloatPack32 = 73, B10G11R11_UFloatPack32 = 74, A2B10G10R10_UNormPack32 = 75,
        A2B10G10R10_UIntPack32 = 76, A2B10G10R10_SIntPack32 = 77, A2R10G10B10_UNormPack32 = 78,
        A2R10G10B10_UIntPack32 = 79, A2R10G10B10_SIntPack32 = 80, A2R10G10B10_XRSRGBPack32 = 81,
        A2R10G10B10_XRUNormPack32 = 82, R10G10B10_XRSRGBPack32 = 83, R10G10B10_XRUNormPack32 = 84,
        A10R10G10B10_XRSRGBPack32 = 85, A10R10G10B10_XRUNormPack32 = 86, D16_UNorm = 90, D24_UNorm = 91,
        D24_UNorm_S8_UInt = 92, D32_SFloat = 93, D32_SFloat_S8_UInt = 94, S8_UInt = 95, RGB_DXT1_SRGB = 96,
        RGBA_DXT1_SRGB = 96, RGB_DXT1_UNorm = 97, RGBA_DXT1_UNorm = 97, RGBA_DXT3_SRGB = 98, RGBA_DXT3_UNorm
        = 99, RGBA_DXT5_SRGB = 100, RGBA_DXT5_UNorm = 101, R_BC4_UNorm = 102, R_BC4_SNorm = 103,
        RG_BC5_UNorm = 104, RG_BC5_SNorm = 105, RGB_BC6H_UFloat = 106, RGB_BC6H_SFloat = 107, RGBA_BC7_SRGB
        = 108, RGBA_BC7_UNorm = 109, RGB_PVRTC_2Bpp_SRGB = 110, RGB_PVRTC_2Bpp_UNorm = 111,
        RGB_PVRTC_4Bpp_SRGB = 112, RGB_PVRTC_4Bpp_UNorm = 113, RGBA_PVRTC_2Bpp_SRGB = 114,
        RGBA_PVRTC_2Bpp_UNorm = 115, RGBA_PVRTC_4Bpp_SRGB = 116, RGBA_PVRTC_4Bpp_UNorm = 117, RGB_ETC_UNorm
        = 118, RGB_ETC2_SRGB = 119, RGB_ETC2_UNorm = 120, RGB_A1_ETC2_SRGB = 121, RGB_A1_ETC2_UNorm = 122,
        RGBA_ETC2_SRGB = 123, RGBA_ETC2_UNorm = 124, R_EAC_UNorm = 125, R_EAC_SNorm = 126, RG_EAC_UNorm =
        127, RG_EAC_SNorm = 128, RGBA_ASTC4X4_SRGB = 129, RGBA_ASTC4X4_UNorm = 130, RGBA_ASTC5X5_SRGB = 131,
        RGBA_ASTC5X5_UNorm = 132, RGBA_ASTC6X6_SRGB = 133, RGBA_ASTC6X6_UNorm = 134, RGBA_ASTC8X8_SRGB =
        135, RGBA_ASTC8X8_UNorm = 136, RGBA_ASTC10X10_SRGB = 137, RGBA_ASTC10X10_UNorm = 138,
        RGBA_ASTC12X12_SRGB = 139, RGBA_ASTC12X12_UNorm = 140, YUV2 = 141, DepthAuto = 142, ShadowAuto =
        143, VideoAuto = 144, RGBA_ASTC4X4_UFloat = 145, RGBA_ASTC5X5_UFloat = 146, RGBA_ASTC6X6_UFloat =
        147, RGBA_ASTC8X8_UFloat = 148, RGBA_ASTC10X10_UFloat = 149, RGBA_ASTC12X12_UFloat = 150,
        D16_UNorm_S8_UInt = 151,
    }
}

namespace UnityEngine.Playables
{
    public enum DirectorWrapMode
    {
        Hold = 0, Loop = 1, None = 2,
    }

    public enum PlayableTraversalMode
    {
        Mix = 0, Passthrough = 1,
    }

    public enum PlayState
    {
        Paused = 0, Playing = 1, Delayed = 2,
    }
}

namespace UnityEngine.SceneManagement
{
    public enum LoadSceneMode
    {
        Single = 0, Additive = 1,
    }
}

namespace UnityEngine.TextCore.LowLevel
{
    [System.Flags]
    public enum GlyphLoadFlags
    {
        LOAD_DEFAULT = 0, LOAD_NO_SCALE = 1, LOAD_NO_HINTING = 2, LOAD_RENDER = 4, LOAD_NO_BITMAP = 8,
        LOAD_FORCE_AUTOHINT = 32, LOAD_MONOCHROME = 4096, LOAD_NO_AUTOHINT = 32768, LOAD_COLOR = 1048576,
        LOAD_COMPUTE_METRICS = 2097152, LOAD_BITMAP_METRICS_ONLY = 4194304,
    }

    public enum FontEngineError
    {
        Success = 0, Invalid_File_Path = 1, Invalid_File_Format = 2, Invalid_File_Structure = 3,
        Invalid_File = 4, Invalid_Table = 8, Invalid_Glyph_Index = 16, Invalid_Character_Code = 17,
        Invalid_Pixel_Size = 23, Invalid_Library = 33, Invalid_Face = 35, Invalid_Library_or_Face = 41,
        Atlas_Generation_Cancelled = 100, Invalid_SharedTextureData = 101, OpenTypeLayoutLookup_Mismatch =
        116,
    }

    public enum GlyphRenderMode
    {
        DEFAULT = 0, SMOOTH_HINTED = 4121, SMOOTH = 4117, COLOR_HINTED = 69656, COLOR = 69652, RASTER_HINTED
        = 4122, RASTER = 4118, SDF = 4134, SDF8 = 8230, SDF16 = 16422, SDF32 = 32806, SDFAA_HINTED = 4169,
        SDFAA = 4165,
    }

    public enum GlyphPackingMode
    {
        BestShortSideFit = 0, BestLongSideFit = 1, BestAreaFit = 2, BottomLeftRule = 3, ContactPointRule =
        4,
    }

    [System.Flags]
    public enum FontFeatureLookupFlags
    {
        None = 0, IgnoreLigatures = 4, IgnoreSpacingAdjustments = 256,
    }
}

namespace UnityEngine.UIElements
{
    public enum DispatchMode
    {
        Default = 1, Queued = 1, Immediate = 2,
    }

    public enum TrickleDown
    {
        NoTrickleDown = 0, TrickleDown = 1,
    }

    public enum NavigationDeviceType
    {
        Unknown = 0, Keyboard = 1, NonKeyboard = 2,
    }

    public enum ContextType
    {
        Player = 0, Editor = 1,
    }
}

namespace UnityEngine.VFX
{
    [System.Flags]
    public enum VFXCameraBufferTypes
    {
        None = 0, Depth = 1, Color = 2, Normal = 4,
    }
}
