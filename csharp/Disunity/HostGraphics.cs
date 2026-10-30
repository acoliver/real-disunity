using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace Disunity.Hosting;

/// <summary>The graphics capabilities <see cref="SystemInfo"/> reports; each flag backs the SystemInfo property of that name.</summary>
[Flags]
public enum GraphicsCapabilities : ulong
{
    None = 0,
    Rendering = 1UL << 0, MultiThreaded = 1UL << 1, UVStartsAtTop = 1UL << 2, TiledGPU = 1UL << 3,
    HiddenSurfaceRemovalOnGPU = 1UL << 4, DynamicUniformArrayIndexingInFragmentShaders = 1UL << 5,
    Shadows = 1UL << 6, RawShadowDepthSampling = 1UL << 7, RenderTextures = 1UL << 8, MotionVectors = 1UL << 9,
    RenderToCubemap = 1UL << 10, ImageEffects = 1UL << 11, Textures3D = 1UL << 12, Compressed3DTextures = 1UL << 13,
    ArrayTextures2D = 1UL << 14, RenderTextures3D = 1UL << 15, CubemapArrayTextures = 1UL << 16,
    AnisotropicFilter = 1UL << 17, ComputeShaders = 1UL << 18, GeometryShaders = 1UL << 19,
    TessellationShaders = 1UL << 20, RenderTargetArrayIndexFromVertexShader = 1UL << 21, Instancing = 1UL << 22,
    HardwareQuadTopology = 1UL << 23, Index32Bit = 1UL << 24, SparseTextures = 1UL << 25,
    SeparatedRenderTargetsBlend = 1UL << 26, Multisampled2DArrayTextures = 1UL << 27,
    MultisampledBackBuffer = 1UL << 28, MemorylessTextures = 1UL << 29, MultisampleAutoResolve = 1UL << 30,
    MultisampledShaderResolve = 1UL << 31, ReversedZBuffer = 1UL << 32, VariableRateShading = 1UL << 33,
    DynamicResolution = 1UL << 34, AsyncCompute = 1UL << 35, GpuRecorder = 1UL << 36, GraphicsFence = 1UL << 37,
    AsyncGPUReadback = 1UL << 38, ParallelPSOCreation = 1UL << 39, RayTracingShaders = 1UL << 40,
    RayTracing = 1UL << 41, InlineRayTracing = 1UL << 42, IndirectDispatchRays = 1UL << 43,
    MachineLearning = 1UL << 44, SetConstantBuffer = 1UL << 45, MinConstantBufferOffsetAlignment = 1UL << 46,
    MipMaxLevel = 1UL << 47, MipStreaming = 1UL << 48, LoadStoreActions = 1UL << 49, ConservativeRaster = 1UL << 50,
    Multiview = 1UL << 51, StoreAndResolveAction = 1UL << 52, MultisampleResolveDepth = 1UL << 53,
    MultisampleResolveStencil = 1UL << 54, IndirectArgumentsBuffer = 1UL << 55, DepthFetchInRenderPass = 1UL << 56,
    VertexPrograms = 1UL << 57,
}

/// <summary>
/// The graphics device a host registers with <see cref="DisunityHost.SetGraphicsDevice"/>. A property left out
/// reads as the null device's value: false, 0 or the empty set. <see cref="Limits"/> holds the integer limits
/// that have no property of their own, keyed by the SystemInfo property name (for example "maxCubemapSize");
/// a missing key reads 0.
/// </summary>
public sealed record GraphicsDeviceInfo
{
    public GraphicsDeviceType Type { get; init; } = GraphicsDeviceType.Null;
    public string Name { get; init; } = "Null Device";
    public string Vendor { get; init; } = "Unknown";
    public int VendorId { get; init; }
    public int DeviceId { get; init; }
    public string Version { get; init; } = "Null Device";
    public int MemoryMB { get; init; }
    public int ShaderLevel { get; init; }
    public int MaxTextureSize { get; init; }
    public int MaxMsaaSamples { get; init; } = 1;
    public GraphicsCapabilities Capabilities { get; init; }
    public NPOTSupport NpotSupport { get; init; }
    public CopyTextureSupport CopyTextureSupport { get; init; }
    public RenderingThreadingMode ThreadingMode { get; init; }
    public IReadOnlyDictionary<string, int>? Limits { get; init; }

    /// <summary>Whether the device supports a format for a usage; null means no format is supported.</summary>
    public Func<GraphicsFormat, GraphicsFormatUsage, bool>? FormatSupport { get; init; }

    /// <summary>Whether the device accepts a vertex attribute format with a component count; null means none.</summary>
    public Func<VertexAttributeFormat, int, bool>? VertexAttributeSupport { get; init; }

    /// <summary>What Unity reports for a player started without graphics (-nographics): the null device. The
    /// engine reports every capability as absent because it draws nothing, the conservative answer where
    /// Unity's own values are not documented.</summary>
    public static GraphicsDeviceInfo Null { get; } = new();

    /// <summary>
    /// The device a windowless player host reports: still the null device, with no capabilities, because nothing
    /// draws, but with the texture limits the engine's renderer guarantees on every GPU adapter it accepts (8192 for
    /// 2D textures and cubemaps, 2048 for 3D textures, 256 array layers, 16x anisotropy). Render pipelines size their
    /// atlases and allocators from these limits when they are created, so zero limits would make those sizes invalid.
    /// </summary>
    public static GraphicsDeviceInfo Windowless { get; } = new()
    {
        MaxTextureSize = 8192,
        Limits = new Dictionary<string, int>
        {
            ["maxCubemapSize"] = 8192,
            ["maxTexture3DSize"] = 2048,
            ["maxTextureArraySlices"] = 256,
            ["maxAnisotropyLevel"] = 16,
        },
    };
}

public static partial class DisunityHost
{
    private static GraphicsDeviceInfo _graphicsDevice = GraphicsDeviceInfo.Null;

    /// <summary>Registers the device <see cref="SystemInfo"/> describes; null restores the null device. Like the
    /// shader registry it outlives worlds.</summary>
    public static void SetGraphicsDevice(GraphicsDeviceInfo? device) => Volatile.Write(ref _graphicsDevice, device ?? GraphicsDeviceInfo.Null);

    internal static GraphicsDeviceInfo GraphicsDevice => Volatile.Read(ref _graphicsDevice);

    /// <summary>Sets the project's default render pipeline asset and the asset of the active quality level;
    /// <see cref="GraphicsSettings.currentRenderPipeline"/> is the quality asset when set, else the default.</summary>
    public static void SetRenderPipeline(RenderPipelineAsset? defaultAsset, RenderPipelineAsset? qualityAsset) =>
        GraphicsSettings.SetPipelineAssets(defaultAsset, qualityAsset);

    /// <summary>Adds a group of settings to the registry <see cref="GraphicsSettings.GetRenderPipelineSettings{T}"/>
    /// reads, replacing an earlier group of the same type.</summary>
    public static void RegisterPipelineSettings(IRenderPipelineGraphicsSettings settings) => GraphicsSettings.AddGraphicsSettings(settings);
}
