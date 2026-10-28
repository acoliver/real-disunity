// Unity enums the player's scripts and packages use, generated from a Unity 6000.3 player's
// engine module metadata with their names and values. The engine reads few of them yet.

namespace UnityEngine.Rendering
{
    [System.Flags]
    public enum RayTracingAccelerationStructureBuildFlags
    {
        None = 0, PreferFastTrace = 1, PreferFastBuild = 2, MinimizeMemory = 4,
    }

    public enum IndexFormat
    {
        UInt16 = 0, UInt32 = 1,
    }

    [System.Flags]
    public enum MeshUpdateFlags
    {
        Default = 0, DontValidateIndices = 1, DontResetBoneBounds = 2, DontNotifyMeshUsers = 4,
        DontRecalculateBounds = 8, DontValidateLodRanges = 16,
    }

    public enum VertexAttributeFormat
    {
        Float32 = 0, Float16 = 1, UNorm8 = 2, SNorm8 = 3, UNorm16 = 4, SNorm16 = 5, UInt8 = 6, SInt8 = 7,
        UInt16 = 8, SInt16 = 9, UInt32 = 10, SInt32 = 11,
    }

    public enum VertexAttribute
    {
        Position = 0, Normal = 1, Tangent = 2, Color = 3, TexCoord0 = 4, TexCoord1 = 5, TexCoord2 = 6,
        TexCoord3 = 7, TexCoord4 = 8, TexCoord5 = 9, TexCoord6 = 10, TexCoord7 = 11, BlendWeight = 12,
        BlendIndices = 13,
    }

    public enum OpaqueSortMode
    {
        Default = 0, FrontToBack = 1, NoDistanceSort = 2,
    }

    public enum RenderBufferLoadAction
    {
        Load = 0, Clear = 1, DontCare = 2,
    }

    public enum RenderBufferStoreAction
    {
        Store = 0, Resolve = 1, StoreAndResolve = 2, DontCare = 3,
    }

    [System.Flags]
    public enum FastMemoryFlags
    {
        None = 0, SpillTop = 1, SpillBottom = 2,
    }

    public enum BlendMode
    {
        Zero = 0, One = 1, DstColor = 2, SrcColor = 3, OneMinusDstColor = 4, SrcAlpha = 5, OneMinusSrcColor
        = 6, DstAlpha = 7, OneMinusDstAlpha = 8, SrcAlphaSaturate = 9, OneMinusSrcAlpha = 10,
    }

    public enum BlendOp
    {
        Add = 0, Subtract = 1, ReverseSubtract = 2, Min = 3, Max = 4, LogicalClear = 5, LogicalSet = 6,
        LogicalCopy = 7, LogicalCopyInverted = 8, LogicalNoop = 9, LogicalInvert = 10, LogicalAnd = 11,
        LogicalNand = 12, LogicalOr = 13, LogicalNor = 14, LogicalXor = 15, LogicalEquivalence = 16,
        LogicalAndReverse = 17, LogicalAndInverted = 18, LogicalOrReverse = 19, LogicalOrInverted = 20,
        Multiply = 21, Screen = 22, Overlay = 23, Darken = 24, Lighten = 25, ColorDodge = 26, ColorBurn =
        27, HardLight = 28, SoftLight = 29, Difference = 30, Exclusion = 31, HSLHue = 32, HSLSaturation =
        33, HSLColor = 34, HSLLuminosity = 35,
    }

    public enum CompareFunction
    {
        Disabled = 0, Never = 1, Less = 2, Equal = 3, LessEqual = 4, Greater = 5, NotEqual = 6, GreaterEqual
        = 7, Always = 8,
    }

    public enum CullMode
    {
        Off = 0, Front = 1, Back = 2,
    }

    [System.Flags]
    public enum ColorWriteMask
    {
        Alpha = 1, Blue = 2, Green = 4, Red = 8, All = 15,
    }

    public enum StencilOp
    {
        Keep = 0, Zero = 1, Replace = 2, IncrementSaturate = 3, DecrementSaturate = 4, Invert = 5,
        IncrementWrap = 6, DecrementWrap = 7,
    }

    public enum CameraEvent
    {
        BeforeDepthTexture = 0, AfterDepthTexture = 1, BeforeDepthNormalsTexture = 2,
        AfterDepthNormalsTexture = 3, BeforeGBuffer = 4, AfterGBuffer = 5, BeforeLighting = 6, AfterLighting
        = 7, BeforeFinalPass = 8, AfterFinalPass = 9, BeforeForwardOpaque = 10, AfterForwardOpaque = 11,
        BeforeImageEffectsOpaque = 12, AfterImageEffectsOpaque = 13, BeforeSkybox = 14, AfterSkybox = 15,
        BeforeForwardAlpha = 16, AfterForwardAlpha = 17, BeforeImageEffects = 18, AfterImageEffects = 19,
        AfterEverything = 20, BeforeReflections = 21, AfterReflections = 22, BeforeHaloAndLensFlares = 23,
        AfterHaloAndLensFlares = 24,
    }

    public enum LightEvent
    {
        BeforeShadowMap = 0, AfterShadowMap = 1, BeforeScreenspaceMask = 2, AfterScreenspaceMask = 3,
        BeforeShadowMapPass = 4, AfterShadowMapPass = 5,
    }

    public enum BuiltinRenderTextureType
    {
        PropertyName = -4, BufferPtr = -3, RenderTexture = -2, BindableTexture = -1, None = 0, CurrentActive
        = 1, CameraTarget = 2, Depth = 3, DepthNormals = 4, ResolvedDepth = 5, PrepassNormalsSpec = 7,
        PrepassLight = 8, PrepassLightSpec = 9, GBuffer0 = 10, GBuffer1 = 11, GBuffer2 = 12, GBuffer3 = 13,
        Reflections = 14, MotionVectors = 15, GBuffer4 = 16, GBuffer5 = 17, GBuffer6 = 18, GBuffer7 = 19,
    }

    public enum ShadowCastingMode
    {
        Off = 0, On = 1, TwoSided = 2, ShadowsOnly = 3,
    }

    public enum LightShadowResolution
    {
        FromQualitySettings = -1, Low = 0, Medium = 1, High = 2, VeryHigh = 3,
    }

    public enum LightUnit
    {
        Lumen = 0, Candela = 1, Lux = 2, Nits = 3, Ev100 = 4,
    }

    public enum GraphicsDeviceType
    {
        OpenGL2 = 0, Direct3D9 = 1, Direct3D11 = 2, PlayStation3 = 3, Null = 4, Xbox360 = 6, OpenGLES2 = 8,
        OpenGLES3 = 11, PlayStationVita = 12, PlayStation4 = 13, XboxOne = 14, PlayStationMobile = 15, Metal
        = 16, OpenGLCore = 17, Direct3D12 = 18, N3DS = 19, Vulkan = 21, Switch = 22, XboxOneD3D12 = 23,
        GameCoreXboxOne = 24, GameCoreScarlett = -1, GameCoreXboxSeries = 25, PlayStation5 = 26,
        PlayStation5NGGC = 27, WebGPU = 28, Switch2 = 29,
    }

    public enum GraphicsTier
    {
        Tier1 = 0, Tier2 = 1, Tier3 = 2,
    }

    public enum FormatSwizzle
    {
        FormatSwizzleR = 0, FormatSwizzleG = 1, FormatSwizzleB = 2, FormatSwizzleA = 3, FormatSwizzle0 = 4,
        FormatSwizzle1 = 5,
    }

    public enum ReflectionProbeMode
    {
        Baked = 0, Realtime = 1, Custom = 2,
    }

    public enum ReflectionProbeRefreshMode
    {
        OnAwake = 0, EveryFrame = 1, ViaScripting = 2,
    }

    public enum ShadowSamplingMode
    {
        CompareDepths = 0, RawDepth = 1, None = 2,
    }

    public enum LightProbeUsage
    {
        Off = 0, BlendProbes = 1, UseProxyVolume = 2, CustomProvided = 4,
    }

    public enum BuiltinShaderType
    {
        DeferredShading = 0, DeferredReflections = 1, LegacyDeferredLighting = 2, ScreenSpaceShadows = 3,
        DepthNormals = 4, MotionVectors = 5, LightHalo = 6, LensFlare = 7,
    }

    public enum BuiltinShaderMode
    {
        Disabled = 0, UseBuiltin = 1, UseCustom = 2,
    }

    public enum BuiltinShaderDefine
    {
        UNITY_NO_DXT5nm = 0, UNITY_NO_RGBM = 1, UNITY_USE_NATIVE_HDR = 2, UNITY_ENABLE_REFLECTION_BUFFERS =
        3, UNITY_FRAMEBUFFER_FETCH_AVAILABLE = 4, UNITY_ENABLE_NATIVE_SHADOW_LOOKUPS = 5,
        UNITY_METAL_SHADOWS_USE_POINT_FILTERING = 6, UNITY_NO_CUBEMAP_ARRAY = 7,
        UNITY_NO_SCREENSPACE_SHADOWS = 8, UNITY_USE_DITHER_MASK_FOR_ALPHABLENDED_SHADOWS = 9,
        UNITY_PBS_USE_BRDF1 = 10, UNITY_PBS_USE_BRDF2 = 11, UNITY_PBS_USE_BRDF3 = 12,
        UNITY_NO_FULL_STANDARD_SHADER = 13, UNITY_SPECCUBE_BOX_PROJECTION = 14, UNITY_SPECCUBE_BLENDING =
        15, UNITY_ENABLE_DETAIL_NORMALMAP = 16, SHADER_API_MOBILE = 17, SHADER_API_DESKTOP = 18,
        UNITY_HARDWARE_TIER1 = 19, UNITY_HARDWARE_TIER2 = 20, UNITY_HARDWARE_TIER3 = 21,
        UNITY_COLORSPACE_GAMMA = 22, UNITY_LIGHT_PROBE_PROXY_VOLUME = 23,
        UNITY_HALF_PRECISION_FRAGMENT_SHADER_REGISTERS = 24, UNITY_LIGHTMAP_DLDR_ENCODING = 25,
        UNITY_LIGHTMAP_RGBM_ENCODING = 26, UNITY_LIGHTMAP_FULL_HDR = 27, UNITY_VIRTUAL_TEXTURING = 28,
        UNITY_PRETRANSFORM_TO_DISPLAY_ORIENTATION = 29, UNITY_ASTC_NORMALMAP_ENCODING = 30,
        SHADER_API_GLES30 = 31, SHADER_API_GLES31 = 32, SHADER_API_GLES32 = 33,
        UNITY_UNIFIED_SHADER_PRECISION_MODEL = 34, UNITY_PLATFORM_SUPPORTS_WAVE_32 = 35,
        UNITY_PLATFORM_SUPPORTS_WAVE_64 = 36, UNITY_NEEDS_RENDERPASS_FBFETCH_FALLBACK = 37,
        UNITY_PLATFORM_SUPPORTS_DEPTH_FETCH = 38,
    }

    public enum TextureDimension
    {
        Unknown = -1, None = 0, Any = 1, Tex2D = 2, Tex3D = 3, Cube = 4, Tex2DArray = 5, CubeArray = 6,
    }

    [System.Flags]
    public enum CopyTextureSupport
    {
        None = 0, Basic = 1, Copy3D = 2, DifferentTypes = 4, TextureToRT = 8, RTToTexture = 16,
    }

    public enum ComputeQueueType
    {
        Default = 0, Background = 1, Urgent = 2,
    }

    public enum SinglePassStereoMode
    {
        None = 0, SideBySide = 1, Instancing = 2, Multiview = 3,
    }

    [System.Flags]
    public enum FoveatedRenderingCaps
    {
        None = 0, FoveationImage = 1, NonUniformRaster = 2, ModeChangeOnlyBeforeRenderTargetSet = 4,
    }

    public enum FoveatedRenderingMode
    {
        Disabled = 0, Enabled = 1,
    }

    public enum CommandBufferExecutionFlags
    {
        None = 0, AsyncCompute = 2,
    }

    [System.Flags]
    public enum RTClearFlags
    {
        None = 0, Color = 1, Depth = 2, Stencil = 4, All = 7, DepthStencil = 6, ColorDepth = 3, ColorStencil
        = 5, Color0 = 8, Color1 = 16, Color2 = 32, Color3 = 64, Color4 = 128, Color5 = 256, Color6 = 512,
        Color7 = 1024,
    }

    public enum RenderTextureSubElement
    {
        Color = 0, Depth = 1, Stencil = 2, Default = 3, ShadingRate = 4,
    }

    public enum CameraLateLatchMatrixType
    {
        View = 0, InverseView = 1, ViewProjection = 2, InverseViewProjection = 3,
    }

    public enum OpenGLESVersion
    {
        None = 0, OpenGLES20 = 1, OpenGLES30 = 2, OpenGLES31 = 3, OpenGLES31AEP = 4, OpenGLES32 = 5,
    }

    public enum ShadingRateFragmentSize
    {
        FragmentSize1x1 = 0, FragmentSize1x2 = 1, FragmentSize2x1 = 2, FragmentSize2x2 = 3, FragmentSize1x4
        = 4, FragmentSize4x1 = 5, FragmentSize2x4 = 6, FragmentSize4x2 = 7, FragmentSize4x4 = 8,
    }

    public enum ShadingRateCombinerStage
    {
        Primitive = 0, Fragment = 1,
    }

    public enum ShadingRateCombiner
    {
        Keep = 0, Override = 1, Min = 2, Max = 3,
    }

    public enum SynchronisationStageFlags
    {
        VertexProcessing = 1, PixelProcessing = 2, ComputeProcessing = 4, AllGPUOperations = 7,
    }

    public enum GraphicsFenceType
    {
        AsyncQueueSynchronisation = 0, CPUSynchronisation = 1,
    }

    [System.Flags]
    public enum RayTracingSubMeshFlags
    {
        Disabled = 0, Enabled = 1, ClosestHitOnly = 2, UniqueAnyHitCalls = 4,
    }

    public enum BatchDrawCommandType
    {
        Direct = 0, Indirect = 1, Procedural = 2, ProceduralIndirect = 3,
    }

    [System.Flags]
    public enum BatchDrawCommandFlags
    {
        None = 0, FlipWinding = 1, HasMotion = 2, IsLightMapped = 4, HasSortingPosition = 8,
        LODCrossFadeKeyword = 16, LODCrossFadeValuePacked = 32, LODCrossFade = 48, UseLegacyLightmapsKeyword
        = 64,
    }

    [System.Flags]
    public enum BatchCullingFlags
    {
        None = 0, CullLightmappedShadowCasters = 1,
    }

    public enum BatchCullingViewType
    {
        Unknown = 0, Camera = 1, Light = 2, Picking = 3, SelectionOutline = 4, Filtering = 5,
    }

    public enum BatchCullingProjectionType
    {
        Unknown = 0, Perspective = 1, Orthographic = 2,
    }

    public enum BatchBufferTarget
    {
        Unknown = 0, UnsupportedByUnderlyingGraphicsApi = -1, RawBuffer = 1, ConstantBuffer = 2,
    }

    [System.Flags]
    public enum SubPassFlags
    {
        None = 0, ReadOnlyDepth = 2, ReadOnlyStencil = 4, ReadOnlyDepthStencil = 6, UseShadingRateImage = 8,
        TileProperties = 16, MultiviewRenderRegionsCompatible = 32,
    }

    [System.Flags]
    public enum CullingOptions
    {
        None = 0, ForceEvenIfCameraIsNotActive = 1, OcclusionCull = 2, NeedsLighting = 4,
        NeedsReflectionProbes = 8, Stereo = 16, DisablePerObjectCulling = 32, ShadowCasters = 64,
    }

    public enum GizmoSubset
    {
        PreImageEffects = 0, PostImageEffects = 1,
    }

    [System.Flags]
    public enum PerObjectData
    {
        None = 0, LightProbe = 1, ReflectionProbes = 2, LightProbeProxyVolume = 4, Lightmaps = 8, LightData
        = 16, MotionVectors = 32, LightIndices = 64, ReflectionProbeData = 128, OcclusionProbe = 256,
        OcclusionProbeProxyVolume = 512, ShadowMask = 1024,
    }

    public enum ReflectionProbeSortingCriteria
    {
        None = 0, Importance = 1, Size = 2, ImportanceThenSize = 3,
    }

    public enum RendererListStatus
    {
        kRendererListInvalid = -2, kRendererListProcessing = -1, kRendererListEmpty = 0,
        kRendererListPopulated = 1,
    }

    [System.Flags]
    public enum RenderStateMask
    {
        Nothing = 0, Blend = 1, Raster = 2, Depth = 4, Stencil = 8, Everything = 15,
    }

    [System.Flags]
    public enum SortingCriteria
    {
        None = 0, SortingLayer = 1, RenderQueue = 2, BackToFront = 4, QuantizedFrontToBack = 8,
        OptimizeStateChanges = 16, CanvasOrder = 32, RendererPriority = 64, CommonOpaque = 59,
        CommonTransparent = 23,
    }

    public enum DistanceMetric
    {
        Perspective = 0, Orthographic = 1, CustomAxis = 2,
    }

    [System.Flags]
    public enum UISubset
    {
        UIToolkit_UGUI = 1, LowLevel = 2, All = -1,
    }
}
