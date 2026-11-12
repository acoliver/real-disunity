namespace UnityEngine;

public enum ColorSpace
{
    Uninitialized = -1,
    Gamma = 0,
    Linear = 1,
}

public enum ColorGamut
{
    sRGB = 0,
}

[Flags]
public enum HDRDisplaySupportFlags
{
    None = 0,
}

public enum FilterMode
{
    Point = 0,
    Bilinear = 1,
    Trilinear = 2,
}

public enum TextureWrapMode
{
    Repeat = 0,
    Clamp = 1,
    Mirror = 2,
    MirrorOnce = 3,
}

public enum TextureFormat
{
    Alpha8 = 1,
    ARGB4444 = 2,
    RGB24 = 3,
    RGBA32 = 4,
    ARGB32 = 5,
    RGB565 = 7,
    R16 = 9,
    DXT1 = 10,
    DXT5 = 12,
    RGBA4444 = 13,
    BGRA32 = 14,
    RHalf = 15,
    RGHalf = 16,
    RGBAHalf = 17,
    RFloat = 18,
    RGFloat = 19,
    RGBAFloat = 20,
    YUY2 = 21,
    RGB9e5Float = 22,
    BC6H = 24,
    BC7 = 25,
    BC4 = 26,
    BC5 = 27,
    DXT1Crunched = 28,
    DXT5Crunched = 29,
    PVRTC_RGB2 = 30,
    PVRTC_RGBA2 = 31,
    PVRTC_RGB4 = 32,
    PVRTC_RGBA4 = 33,
    ETC_RGB4 = 34,
    EAC_R = 41,
    EAC_R_SIGNED = 42,
    EAC_RG = 43,
    EAC_RG_SIGNED = 44,
    ETC2_RGB = 45,
    ETC2_RGBA1 = 46,
    ETC2_RGBA8 = 47,
    ASTC_4x4 = 48,
    ASTC_5x5 = 49,
    ASTC_6x6 = 50,
    ASTC_8x8 = 51,
    ASTC_10x10 = 52,
    ASTC_12x12 = 53,
    RG16 = 62,
    R8 = 63,
    ETC_RGB4Crunched = 64,
    ETC2_RGBA8Crunched = 65,
    ASTC_HDR_4x4 = 66,
    ASTC_HDR_5x5 = 67,
    ASTC_HDR_6x6 = 68,
    ASTC_HDR_8x8 = 69,
    ASTC_HDR_10x10 = 70,
    ASTC_HDR_12x12 = 71,
    RG32 = 72,
    RGB48 = 73,
    RGBA64 = 74,
    R8_SIGNED = 75,
    RG16_SIGNED = 76,
    RGB24_SIGNED = 77,
    RGBA32_SIGNED = 78,
    R16_SIGNED = 79,
    RG32_SIGNED = 80,
    RGB48_SIGNED = 81,
    RGBA64_SIGNED = 82,
}

public enum RenderTextureFormat
{
    ARGB32 = 0,
    Depth = 1,
    ARGBHalf = 2,
    Shadowmap = 3,
    RGB565 = 4,
    ARGB4444 = 5,
    ARGB1555 = 6,
    Default = 7,
    ARGB2101010 = 8,
    DefaultHDR = 9,
    ARGB64 = 10,
    ARGBFloat = 11,
    RGFloat = 12,
    RGHalf = 13,
    RFloat = 14,
    RHalf = 15,
    R8 = 16,
    ARGBInt = 17,
    RGInt = 18,
    RInt = 19,
    BGRA32 = 20,
    RGB111110Float = 22,
    RG32 = 23,
    RG16 = 25,
    R16 = 28,
}

public enum RenderTextureReadWrite
{
    Default = 0,
    Linear = 1,
    sRGB = 2,
}

[Flags]
public enum RenderTextureMemoryless
{
    None = 0,
    Color = 1,
    Depth = 2,
    MSAA = 4,
}

public enum VRTextureUsage
{
    None = 0,
    OneEye = 1,
    TwoEyes = 2,
    DeviceSpecific = 3,
}

public enum CubemapFace
{
    Unknown = -1,
    PositiveX = 0,
    NegativeX = 1,
    PositiveY = 2,
    NegativeY = 3,
    PositiveZ = 4,
    NegativeZ = 5,
}

[Flags]
public enum DepthTextureMode
{
    None = 0,
    Depth = 1,
    DepthNormals = 2,
    MotionVectors = 4,
}

public enum CameraType
{
    Game = 1,
    SceneView = 2,
    Preview = 4,
    VR = 8,
    Reflection = 16,
}

public enum RenderingPath
{
    UsePlayerSettings = -1,
    VertexLit = 0,
    Forward = 1,
    DeferredLighting = 2,
    DeferredShading = 3,
}

[Flags]
public enum StereoTargetEyeMask
{
    None = 0,
    Left = 1,
    Right = 2,
    Both = Left | Right,
}

public enum TransparencySortMode
{
    Default = 0,
    Perspective = 1,
    Orthographic = 2,
    CustomAxis = 3,
}

public enum MeshTopology
{
    Triangles = 0,
    Quads = 2,
    Lines = 3,
    LineStrip = 4,
    Points = 5,
}

public enum SkinQuality
{
    Auto = 0,
    Bone1 = 1,
    Bone2 = 2,
    Bone4 = 4,
}

public enum MotionVectorGenerationMode
{
    Camera = 0,
    Object = 1,
    ForceNoMotion = 2,
}

public enum SpriteDrawMode
{
    Simple = 0,
    Sliced = 1,
    Tiled = 2,
}

public enum SpriteMaskInteraction
{
    None = 0,
    VisibleInsideMask = 1,
    VisibleOutsideMask = 2,
}

public enum SpriteMeshType
{
    FullRect = 0,
    Tight = 1,
}

public enum LightType
{
    Spot = 0,
    Directional = 1,
    Point = 2,
}

public enum LightShadows
{
    None = 0,
    Hard = 1,
    Soft = 2,
}

[Flags]
public enum LightmapBakeType
{
    Realtime = 4,
    Baked = 2,
    Mixed = 1,
}

public enum MixedLightingMode
{
    IndirectOnly = 0,
    Subtractive = 1,
    Shadowmask = 2,
}

public enum ShadowQuality
{
    Disable = 0,
    HardOnly = 1,
    All = 2,
}

public enum ShadowmaskMode
{
    Shadowmask = 0,
    DistanceShadowmask = 1,
}

[Flags]
public enum ComputeBufferType
{
    Default = 0,
    Raw = 1,
    Append = 2,
    Counter = 4,
    Constant = 8,
    Structured = 16,
    DrawIndirect = 256,
    IndirectArguments = 256,
}

public enum ComputeBufferMode
{
    Immutable = 0,
    Dynamic = 1,
    Circular = 2,
    StreamOut = 3,
    SubUpdates = 4,
}
