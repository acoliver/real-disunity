// Unity enums the player's scripts and packages use, generated from a Unity 6000.3 player's
// engine module metadata with their names and values. The engine reads few of them yet.

namespace UnityEngine
{
    public enum SpriteDrawMode
    {
        Simple = 0, Sliced = 1, Tiled = 2,
    }

    public enum SpriteMaskInteraction
    {
        None = 0, VisibleInsideMask = 1, VisibleOutsideMask = 2,
    }

    public enum SpriteMeshType
    {
        FullRect = 0, Tight = 1,
    }

    public enum WeightedMode
    {
        None = 0, In = 1, Out = 2, Both = 3,
    }

    public enum WrapMode
    {
        Once = 1, Loop = 2, PingPong = 4, Default = 0, ClampForever = 8, Clamp = 1,
    }

    public enum SendMessageOptions
    {
        RequireReceiver = 0, DontRequireReceiver = 1,
    }

    public enum PrimitiveType
    {
        Sphere = 0, Capsule = 1, Cylinder = 2, Cube = 3, Plane = 4, Quad = 5,
    }

    public enum Space
    {
        World = 0, Self = 1,
    }

    public enum FullScreenMode
    {
        ExclusiveFullScreen = 0, FullScreenWindow = 1, MaximizedWindow = 2, Windowed = 3,
    }

    public enum ComputeBufferMode
    {
        Immutable = 0, Dynamic = 1, Circular = 2, StreamOut = 3, SubUpdates = 4,
    }

    public enum RenderingPath
    {
        UsePlayerSettings = -1, VertexLit = 0, Forward = 1, DeferredLighting = 2, DeferredShading = 3,
    }

    public enum TransparencySortMode
    {
        Default = 0, Perspective = 1, Orthographic = 2, CustomAxis = 3,
    }

    public enum StereoTargetEyeMask
    {
        None = 0, Left = 1, Right = 2, Both = 3,
    }

    [System.Flags]
    public enum CameraType
    {
        Game = 1, SceneView = 2, Preview = 4, VR = 8, Reflection = 16,
    }

    [System.Flags]
    public enum ComputeBufferType
    {
        Default = 0, Raw = 1, Append = 2, Counter = 4, Constant = 8, Structured = 16, DrawIndirect = 256,
        IndirectArguments = 256, GPUMemory = 512,
    }

    public enum LightType
    {
        Spot = 0, Directional = 1, Point = 2, Area = 3, Rectangle = 3, Disc = 4, Pyramid = 5, Box = 6, Tube
        = 7,
    }

    public enum LightShadows
    {
        None = 0, Hard = 1, Soft = 2,
    }

    [System.Flags]
    public enum LightmapBakeType
    {
        Realtime = 4, Baked = 2, Mixed = 1,
    }

    public enum MixedLightingMode
    {
        IndirectOnly = 0, Shadowmask = 2, Subtractive = 1,
    }

    public enum ShadowQuality
    {
        Disable = 0, HardOnly = 1, All = 2,
    }

    public enum ShadowmaskMode
    {
        Shadowmask = 0, DistanceShadowmask = 1,
    }

    [System.Flags]
    public enum DepthTextureMode
    {
        None = 0, Depth = 1, DepthNormals = 2, MotionVectors = 4,
    }

    public enum MeshTopology
    {
        Triangles = 0, Quads = 2, Lines = 3, LineStrip = 4, Points = 5,
    }

    public enum ColorSpace
    {
        Uninitialized = -1, Gamma = 0, Linear = 1,
    }

    public enum ColorGamut
    {
        sRGB = 0, Rec709 = 1, Rec2020 = 2, DisplayP3 = 3, HDR10 = 4, DolbyHDR = 5, P3D65G22 = 6,
    }

    public enum ColorPrimaries
    {
        Unknown = -1, Rec709 = 0, Rec2020 = 1, P3 = 2,
    }

    public enum WhitePoint
    {
        Unknown = -1, D65 = 0,
    }

    public enum TransferFunction
    {
        Unknown = -1, sRGB = 0, BT1886 = 1, PQ = 2, Linear = 3, Gamma22 = 4,
    }

    public enum FilterMode
    {
        Point = 0, Bilinear = 1, Trilinear = 2,
    }

    public enum TextureWrapMode
    {
        Repeat = 0, Clamp = 1, Mirror = 2, MirrorOnce = 3,
    }

    public enum TextureFormat
    {
        Alpha8 = 1, ARGB4444 = 2, RGB24 = 3, RGBA32 = 4, ARGB32 = 5, RGB565 = 7, R16 = 9, DXT1 = 10, DXT5 =
        12, RGBA4444 = 13, BGRA32 = 14, RHalf = 15, RGHalf = 16, RGBAHalf = 17, RFloat = 18, RGFloat = 19,
        RGBAFloat = 20, YUY2 = 21, RGB9e5Float = 22, BC4 = 26, BC5 = 27, BC6H = 24, BC7 = 25, DXT1Crunched =
        28, DXT5Crunched = 29, PVRTC_RGB2 = 30, PVRTC_RGBA2 = 31, PVRTC_RGB4 = 32, PVRTC_RGBA4 = 33,
        ETC_RGB4 = 34, EAC_R = 41, EAC_R_SIGNED = 42, EAC_RG = 43, EAC_RG_SIGNED = 44, ETC2_RGB = 45,
        ETC2_RGBA1 = 46, ETC2_RGBA8 = 47, ASTC_4x4 = 48, ASTC_5x5 = 49, ASTC_6x6 = 50, ASTC_8x8 = 51,
        ASTC_10x10 = 52, ASTC_12x12 = 53, RG16 = 62, R8 = 63, ETC_RGB4Crunched = 64, ETC2_RGBA8Crunched =
        65, ASTC_HDR_4x4 = 66, ASTC_HDR_5x5 = 67, ASTC_HDR_6x6 = 68, ASTC_HDR_8x8 = 69, ASTC_HDR_10x10 = 70,
        ASTC_HDR_12x12 = 71, RG32 = 72, RGB48 = 73, RGBA64 = 74, R8_SIGNED = 75, RG16_SIGNED = 76,
        RGB24_SIGNED = 77, RGBA32_SIGNED = 78, R16_SIGNED = 79, RG32_SIGNED = 80, RGB48_SIGNED = 81,
        RGBA64_SIGNED = 82,
    }

    public enum CubemapFace
    {
        Unknown = -1, PositiveX = 0, NegativeX = 1, PositiveY = 2, NegativeY = 3, PositiveZ = 4, NegativeZ =
        5,
    }

    public enum RenderTextureFormat
    {
        ARGB32 = 0, Depth = 1, ARGBHalf = 2, Shadowmap = 3, RGB565 = 4, ARGB4444 = 5, ARGB1555 = 6, Default
        = 7, ARGB2101010 = 8, DefaultHDR = 9, ARGB64 = 10, ARGBFloat = 11, RGFloat = 12, RGHalf = 13, RFloat
        = 14, RHalf = 15, R8 = 16, ARGBInt = 17, RGInt = 18, RInt = 19, BGRA32 = 20, RGB111110Float = 22,
        RG32 = 23, RGBAUShort = 24, RG16 = 25, BGRA10101010_XR = 26, BGR101010_XR = 27, R16 = 28,
    }

    public enum VRTextureUsage
    {
        None = 0, OneEye = 1, TwoEyes = 2, DeviceSpecific = 3,
    }

    public enum RenderTextureReadWrite
    {
        Default = 0, Linear = 1, sRGB = 2,
    }

    [System.Flags]
    public enum RenderTextureMemoryless
    {
        None = 0, Color = 1, Depth = 2, MSAA = 4,
    }

    [System.Flags]
    public enum HDRDisplaySupportFlags
    {
        None = 0, Supported = 1, RuntimeSwitchable = 2, AutomaticTonemapping = 4,
    }

    public enum SkinQuality
    {
        Auto = 0, Bone1 = 1, Bone2 = 2, Bone4 = 4,
    }

    public enum MotionVectorGenerationMode
    {
        Camera = 0, Object = 1, ForceNoMotion = 2,
    }

    public enum LODFadeMode
    {
        None = 0, CrossFade = 1, SpeedTree = 2,
    }

    public enum CursorMode
    {
        Auto = 0, ForceSoftware = 1,
    }

    public enum CursorLockMode
    {
        None = 0, Locked = 1, Confined = 2,
    }

    public enum GradientMode
    {
        Blend = 0, Fixed = 1, PerceptualBlend = 2,
    }

    [System.Flags]
    public enum HideFlags
    {
        None = 0, HideInHierarchy = 1, HideInInspector = 2, DontSaveInEditor = 4, NotEditable = 8,
        DontSaveInBuild = 16, DontUnloadUnusedAsset = 32, DontSave = 52, HideAndDontSave = 61,
    }

    public enum FindObjectsInactive
    {
        Exclude = 0, Include = 1,
    }

    public enum BatteryStatus
    {
        Unknown = 0, Charging = 1, Discharging = 2, NotCharging = 3, Full = 4,
    }

    public enum OperatingSystemFamily
    {
        Other = 0, MacOSX = 1, Windows = 2, Linux = 3,
    }

    public enum DeviceType
    {
        Unknown = 0, Handheld = 1, Console = 2, Desktop = 3,
    }

    public enum TouchScreenKeyboardType
    {
        Default = 0, ASCIICapable = 1, NumbersAndPunctuation = 2, URL = 3, NumberPad = 4, PhonePad = 5,
        NamePhonePad = 6, EmailAddress = 7, NintendoNetworkAccount = 8, Social = 9, Search = 10, DecimalPad
        = 11, OneTimeCode = 12,
    }

    [System.Flags]
    public enum DrivenTransformProperties
    {
        None = 0, All = -1, AnchoredPositionX = 2, AnchoredPositionY = 4, AnchoredPositionZ = 8, Rotation =
        16, ScaleX = 32, ScaleY = 64, ScaleZ = 128, AnchorMinX = 256, AnchorMinY = 512, AnchorMaxX = 1024,
        AnchorMaxY = 2048, SizeDeltaX = 4096, SizeDeltaY = 8192, PivotX = 16384, PivotY = 32768,
        AnchoredPosition = 6, AnchoredPosition3D = 14, Scale = 224, AnchorMin = 768, AnchorMax = 3072,
        Anchors = 3840, SizeDelta = 12288, Pivot = 49152,
    }

    public enum QueryTriggerInteraction
    {
        UseGlobal = 0, Ignore = 1, Collide = 2,
    }

    public enum DurationUnit
    {
        Fixed = 0, Normalized = 1,
    }

    public enum AnimatorCullingMode
    {
        AlwaysAnimate = 0, CullUpdateTransforms = 1, CullCompletely = 2,
    }

    public enum AnimatorUpdateMode
    {
        Normal = 0, Fixed = 1, UnscaledTime = 2,
    }

    public enum AvatarMaskBodyPart
    {
        Root = 0, Body = 1, Head = 2, LeftLeg = 3, RightLeg = 4, LeftArm = 5, RightArm = 6, LeftFingers = 7,
        RightFingers = 8, LeftFootIK = 9, RightFootIK = 10, LeftHandIK = 11, RightHandIK = 12, LastBodyPart
        = 13,
    }

    public enum EventType
    {
        MouseDown = 0, MouseUp = 1, MouseMove = 2, MouseDrag = 3, KeyDown = 4, KeyUp = 5, ScrollWheel = 6,
        Repaint = 7, Layout = 8, DragUpdated = 9, DragPerform = 10, DragExited = 15, Ignore = 11, Used = 12,
        ValidateCommand = 13, ExecuteCommand = 14, ContextClick = 16, MouseEnterWindow = 20,
        MouseLeaveWindow = 21, TouchDown = 30, TouchUp = 31, TouchMove = 32, TouchEnter = 33, TouchLeave =
        34, TouchStationary = 35, mouseDown = 0, mouseUp = 1, mouseMove = 2, mouseDrag = 3, keyDown = 4,
        keyUp = 5, scrollWheel = 6, repaint = 7, layout = 8, dragUpdated = 9, dragPerform = 10, ignore = 11,
        used = 12,
    }

    [System.Flags]
    public enum EventModifiers
    {
        None = 0, Shift = 1, Control = 2, Alt = 4, Command = 8, Numeric = 16, CapsLock = 32, FunctionKey =
        64,
    }

    public enum TouchPhase
    {
        Began = 0, Moved = 1, Stationary = 2, Ended = 3, Canceled = 4,
    }

    public enum IMECompositionMode
    {
        Auto = 0, On = 1, Off = 2,
    }

    public enum TouchType
    {
        Direct = 0, Indirect = 1, Stylus = 2,
    }

    [System.Flags]
    public enum PenStatus
    {
        None = 0, Contact = 1, Barrel = 2, Inverted = 4, Eraser = 8,
    }

    [System.Flags]
    public enum AdditionalCanvasShaderChannels
    {
        None = 0, TexCoord1 = 1, TexCoord2 = 2, TexCoord3 = 4, Normal = 8, Tangent = 16,
    }

    public enum ParticleSystemAnimationType
    {
        WholeSheet = 0, SingleRow = 1,
    }

    public enum ParticleSystemSimulationSpace
    {
        Local = 0, World = 1, Custom = 2,
    }

    public enum ParticleSystemStopBehavior
    {
        StopEmittingAndClear = 0, StopEmitting = 1,
    }

    public enum ParticleSystemScalingMode
    {
        Hierarchy = 0, Local = 1, Shape = 2,
    }

    public enum PhysicsShapeType2D
    {
        Circle = 0, Capsule = 1, Polygon = 2, Edges = 3,
    }
}
