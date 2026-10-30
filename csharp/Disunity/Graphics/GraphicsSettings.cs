namespace UnityEngine.Rendering;

/// <summary>
/// The project's graphics settings, held as static state. Each setting starts at the value a new Unity project
/// has: sort mode Default, sort axis (0, 0, 1), rendering layer mask 1, every light and culling flag off, shader
/// types using the built-in shaders, light probes searching the tetrahedral hull, gate fit Horizontal, and video
/// shaders included when referenced (the last two defaults are the engine's choice). The render pipeline
/// assets are null until a host registers them with
/// DisunityHost.SetRenderPipeline. Render pipeline settings live in the registries in
/// <c>GraphicsSettingsPipelines.cs</c>.
/// </summary>
public sealed partial class GraphicsSettings : Object
{
    private static readonly object Gate = new();
    private static readonly GraphicsSettings Instance = new();
    private static readonly Dictionary<BuiltinShaderType, BuiltinShaderMode> ShaderModes = new();
    private static readonly Dictionary<BuiltinShaderType, Shader?> CustomShaders = new();

    private static Vector3 _sortAxis = new(0f, 0f, 1f);
    private static uint _renderingLayerMask = 1;
    private static Camera.GateFitMode _gateFitMode = Camera.GateFitMode.Horizontal;
    private static RenderPipelineAsset? _defaultPipeline;

    private GraphicsSettings() { }

    public static TransparencySortMode transparencySortMode { get; set; } = TransparencySortMode.Default;
    public static Vector3 transparencySortAxis { get => _sortAxis; set => _sortAxis = value; }
    public static bool realtimeDirectRectangularAreaLights { get; set; }
    public static bool lightsUseLinearIntensity { get; set; }
    public static bool lightsUseColorTemperature { get; set; }
    public static uint defaultRenderingLayerMask { get => _renderingLayerMask; set => _renderingLayerMask = value; }
    public static Camera.GateFitMode defaultGateFitMode { get => _gateFitMode; set => _gateFitMode = value; }
    public static bool useScriptableRenderPipelineBatching { get; set; }
    public static bool logWhenShaderIsCompiled { get; set; }
    public static bool disableBuiltinCustomRenderTextureUpdate { get; set; }
    public static VideoShadersIncludeMode videoShadersIncludeMode => VideoShadersIncludeMode.Referenced;
    public static LightProbeOutsideHullStrategy lightProbeOutsideHullStrategy { get; set; } = LightProbeOutsideHullStrategy.kLightProbeSearchTetrahedralHull;
    public static bool cameraRelativeLightCulling { get; set; }
    public static bool cameraRelativeShadowCulling { get; set; }

    public static RenderPipelineAsset? defaultRenderPipeline
    {
        get { lock (Gate) return _defaultPipeline; }
        set { lock (Gate) _defaultPipeline = value; }
    }

    /// <summary>The same asset as <see cref="defaultRenderPipeline"/>; Unity keeps the older name for it.</summary>
    public static RenderPipelineAsset? renderPipelineAsset
    {
        get => defaultRenderPipeline;
        set => defaultRenderPipeline = value;
    }

    /// <summary>The active quality level's asset when it has one, else the default asset; null with neither.</summary>
    public static RenderPipelineAsset? currentRenderPipeline
    {
        get
        {
            var quality = QualitySettings.renderPipeline;
            lock (Gate) return quality ? quality : _defaultPipeline;
        }
    }

    public static bool isScriptableRenderPipelineEnabled => currentRenderPipeline;

    public static Type? currentRenderPipelineAssetType => currentRenderPipeline?.GetType();

    /// <summary>The default asset and the assets of every quality level that are set, each once.</summary>
    public static RenderPipelineAsset[] allConfiguredRenderPipelines
    {
        get
        {
            var found = new List<RenderPipelineAsset>(QualitySettings.ConfiguredPipelines());
            lock (Gate) if (_defaultPipeline && !found.Contains(_defaultPipeline!)) found.Insert(0, _defaultPipeline!);
            return [.. found];
        }
    }

    internal static void SetPipelineAssets(RenderPipelineAsset? defaultAsset, RenderPipelineAsset? qualityAsset)
    {
        lock (Gate) _defaultPipeline = defaultAsset;
        QualitySettings.renderPipeline = qualityAsset;
    }

    /// <summary>The object Unity's graphics settings live in; here one shared object without settings of its own.</summary>
    public static Object GetGraphicsSettings() => Instance;

    public static void SetShaderMode(BuiltinShaderType type, BuiltinShaderMode mode)
    {
        if (!Enum.IsDefined(type)) throw new ArgumentOutOfRangeException(nameof(type), type, "Unknown built-in shader type");
        if (!Enum.IsDefined(mode)) throw new ArgumentOutOfRangeException(nameof(mode), mode, "Unknown built-in shader mode");
        lock (Gate) ShaderModes[type] = mode;
    }

    public static BuiltinShaderMode GetShaderMode(BuiltinShaderType type)
    {
        lock (Gate) return ShaderModes.TryGetValue(type, out var mode) ? mode : BuiltinShaderMode.UseBuiltin;
    }

    public static void SetCustomShader(BuiltinShaderType type, Shader shader)
    {
        lock (Gate) CustomShaders[type] = shader;
    }

    public static Shader? GetCustomShader(BuiltinShaderType type)
    {
        lock (Gate) return CustomShaders.TryGetValue(type, out var shader) && shader ? shader : null;
    }

    /// <summary>
    /// Whether the platform defines a built-in shader macro. The engine has no shader compiler and no
    /// platform shader headers, so every define is reported as absent for every tier.
    /// </summary>
    public static bool HasShaderDefine(GraphicsTier tier, BuiltinShaderDefine defineHash) => false;

    public static bool HasShaderDefine(BuiltinShaderDefine defineHash) => false;
}
