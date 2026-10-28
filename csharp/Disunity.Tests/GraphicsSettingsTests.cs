using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>GraphicsSettings defaults, round trips, pipeline assets and the settings registries.</summary>
internal static class GraphicsSettingsTests
{
    private sealed class PipelineAsset : RenderPipelineAsset { }

    private sealed class OtherAsset : RenderPipelineAsset { }

    private sealed class GlobalSettings : RenderPipelineGlobalSettings { }

    private sealed class Pipeline : RenderPipeline { }

    private sealed class CloudSettings : IRenderPipelineGraphicsSettings
    {
        public int version => 3;
    }

    private sealed class Unregistered : IRenderPipelineGraphicsSettings
    {
        public int version => 1;
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            RoundTrips();
            ShaderModes();
            Pipelines();
            Registries();
        }
        finally
        {
            DisunityHost.SetRenderPipeline(null, null);
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: GraphicsSettings defaults, round trips, shader modes, pipeline assets and settings registries");
    }

    private static void Defaults()
    {
        Expect.That(GraphicsSettings.transparencySortMode == TransparencySortMode.Default && GraphicsSettings.transparencySortAxis == new Vector3(0, 0, 1), "sort defaults");
        Expect.That(!GraphicsSettings.lightsUseColorTemperature && !GraphicsSettings.lightsUseLinearIntensity && !GraphicsSettings.realtimeDirectRectangularAreaLights, "light flags");
        Expect.That(!GraphicsSettings.useScriptableRenderPipelineBatching && !GraphicsSettings.logWhenShaderIsCompiled && !GraphicsSettings.disableBuiltinCustomRenderTextureUpdate, "batching and logging flags");
        Expect.That(!GraphicsSettings.cameraRelativeLightCulling && !GraphicsSettings.cameraRelativeShadowCulling, "culling flags");
        Expect.Equal(1u, GraphicsSettings.defaultRenderingLayerMask, "defaultRenderingLayerMask");
        Expect.That(GraphicsSettings.defaultGateFitMode == Camera.GateFitMode.Horizontal, "defaultGateFitMode");
        Expect.That(GraphicsSettings.videoShadersIncludeMode == VideoShadersIncludeMode.Referenced, "videoShadersIncludeMode");
        Expect.That(GraphicsSettings.lightProbeOutsideHullStrategy == LightProbeOutsideHullStrategy.kLightProbeSearchTetrahedralHull, "lightProbeOutsideHullStrategy");
        Expect.That(GraphicsSettings.currentRenderPipeline == null && GraphicsSettings.renderPipelineAsset == null && GraphicsSettings.defaultRenderPipeline == null, "no pipeline assets");
        Expect.That(!GraphicsSettings.isScriptableRenderPipelineEnabled && GraphicsSettings.currentRenderPipelineAssetType == null && GraphicsSettings.allConfiguredRenderPipelines.Length == 0, "no pipeline is enabled");
        Expect.That(GraphicsSettings.GetGraphicsSettings() == GraphicsSettings.GetGraphicsSettings(), "one settings object");
    }

    private static void RoundTrips()
    {
        GraphicsSettings.transparencySortMode = TransparencySortMode.CustomAxis;
        GraphicsSettings.transparencySortAxis = new Vector3(1, 0, 0);
        GraphicsSettings.lightsUseColorTemperature = true;
        GraphicsSettings.lightsUseLinearIntensity = true;
        GraphicsSettings.useScriptableRenderPipelineBatching = true;
        GraphicsSettings.defaultRenderingLayerMask = 0xff;
        GraphicsSettings.cameraRelativeLightCulling = true;
        GraphicsSettings.defaultGateFitMode = Camera.GateFitMode.Fill;
        GraphicsSettings.lightProbeOutsideHullStrategy = LightProbeOutsideHullStrategy.kLightProbeUseAmbientProbe;
        Expect.That(GraphicsSettings.transparencySortMode == TransparencySortMode.CustomAxis && GraphicsSettings.transparencySortAxis == new Vector3(1, 0, 0), "sort values are stored");
        Expect.That(GraphicsSettings.lightsUseColorTemperature && GraphicsSettings.lightsUseLinearIntensity && GraphicsSettings.useScriptableRenderPipelineBatching, "flags are stored");
        Expect.That(GraphicsSettings.defaultRenderingLayerMask == 0xff && GraphicsSettings.cameraRelativeLightCulling && GraphicsSettings.defaultGateFitMode == Camera.GateFitMode.Fill, "other values are stored");
        Expect.That(GraphicsSettings.lightProbeOutsideHullStrategy == LightProbeOutsideHullStrategy.kLightProbeUseAmbientProbe, "strategy is stored");
        GraphicsSettings.transparencySortMode = TransparencySortMode.Default;
        GraphicsSettings.transparencySortAxis = new Vector3(0, 0, 1);
        GraphicsSettings.lightsUseColorTemperature = GraphicsSettings.lightsUseLinearIntensity = GraphicsSettings.useScriptableRenderPipelineBatching = false;
        GraphicsSettings.defaultRenderingLayerMask = 1;
        GraphicsSettings.cameraRelativeLightCulling = false;
        GraphicsSettings.defaultGateFitMode = Camera.GateFitMode.Horizontal;
        GraphicsSettings.lightProbeOutsideHullStrategy = LightProbeOutsideHullStrategy.kLightProbeSearchTetrahedralHull;
    }

    private static void ShaderModes()
    {
        Expect.That(GraphicsSettings.GetShaderMode(BuiltinShaderType.LensFlare) == BuiltinShaderMode.UseBuiltin, "built-in by default");
        GraphicsSettings.SetShaderMode(BuiltinShaderType.LensFlare, BuiltinShaderMode.Disabled);
        Expect.That(GraphicsSettings.GetShaderMode(BuiltinShaderType.LensFlare) == BuiltinShaderMode.Disabled, "mode is stored");
        Expect.That(GraphicsSettings.GetShaderMode(BuiltinShaderType.LightHalo) == BuiltinShaderMode.UseBuiltin, "other types are unchanged");
        GraphicsSettings.SetShaderMode(BuiltinShaderType.LensFlare, BuiltinShaderMode.UseBuiltin);
        Expect.Throws<ArgumentOutOfRangeException>(() => GraphicsSettings.SetShaderMode((BuiltinShaderType)99, BuiltinShaderMode.Disabled), "unknown type");
        Expect.That(GraphicsSettings.GetCustomShader(BuiltinShaderType.LightHalo) == null, "no custom shader by default");
        var shader = DisunityHost.RegisterShader("Test/Halo");
        GraphicsSettings.SetCustomShader(BuiltinShaderType.LightHalo, shader);
        Expect.That(GraphicsSettings.GetCustomShader(BuiltinShaderType.LightHalo) == shader, "custom shader is stored");
        Expect.That(!GraphicsSettings.HasShaderDefine(BuiltinShaderDefine.SHADER_API_DESKTOP) && !GraphicsSettings.HasShaderDefine(GraphicsTier.Tier2, BuiltinShaderDefine.UNITY_HARDWARE_TIER2), "no shader defines");
    }

    private static void Pipelines()
    {
        var defaultAsset = ScriptableObject.CreateInstance<PipelineAsset>();
        var quality = ScriptableObject.CreateInstance<OtherAsset>();
        DisunityHost.SetRenderPipeline(defaultAsset, null);
        Expect.That(GraphicsSettings.currentRenderPipeline == defaultAsset && GraphicsSettings.renderPipelineAsset == defaultAsset, "the default is current without a quality asset");
        Expect.That(GraphicsSettings.isScriptableRenderPipelineEnabled && GraphicsSettings.currentRenderPipelineAssetType == typeof(PipelineAsset), "the pipeline is enabled");
        DisunityHost.SetRenderPipeline(defaultAsset, quality);
        Expect.That(GraphicsSettings.currentRenderPipeline == quality && GraphicsSettings.defaultRenderPipeline == defaultAsset, "the quality asset wins");
        Expect.That(GraphicsSettings.currentRenderPipelineAssetType == typeof(OtherAsset), "type of the quality asset");
        Expect.Sequence(new RenderPipelineAsset[] { defaultAsset, quality }, GraphicsSettings.allConfiguredRenderPipelines, "both assets are configured");
        DisunityHost.SetRenderPipeline(defaultAsset, defaultAsset);
        Expect.Equal(1, GraphicsSettings.allConfiguredRenderPipelines.Length, "one asset is listed once");
        GraphicsSettings.renderPipelineAsset = quality;
        Expect.That(GraphicsSettings.defaultRenderPipeline == quality, "renderPipelineAsset sets the default");
        DisunityHost.SetRenderPipeline(null, null);
        Expect.That(GraphicsSettings.currentRenderPipeline == null && GraphicsSettings.allConfiguredRenderPipelines.Length == 0, "assets are cleared");
        Expect.Throws<NotSupportedException>(() => GraphicsSettings.TryGetCurrentRenderPipelineGlobalSettings(out _), "the current pipeline's type is not known yet");
    }

    private static void Registries()
    {
        Expect.That(GraphicsSettings.GetRenderPipelineSettings<CloudSettings>() == null && !GraphicsSettings.TryGetRenderPipelineSettings<CloudSettings>(out var none) && none == null, "nothing is registered");
        var cloud = new CloudSettings();
        DisunityHost.RegisterPipelineSettings(cloud);
        Expect.That(GraphicsSettings.GetRenderPipelineSettings<CloudSettings>() == cloud, "Get finds the group");
        Expect.That(GraphicsSettings.TryGetRenderPipelineSettings<CloudSettings>(out var found) && found == cloud, "TryGet finds the group");
        Expect.That(GraphicsSettings.GetRenderPipelineSettings<Unregistered>() == null, "other groups are absent");
        var replacement = new CloudSettings();
        DisunityHost.RegisterPipelineSettings(replacement);
        Expect.That(GraphicsSettings.GetRenderPipelineSettings<CloudSettings>() == replacement, "a group of the same type is replaced");

        Expect.That(GraphicsSettings.GetSettingsForRenderPipeline<Pipeline>() == null, "no global settings yet");
        var settings = ScriptableObject.CreateInstance<GlobalSettings>();
        GraphicsSettings.RegisterRenderPipelineSettings<Pipeline, GlobalSettings>(settings);
        Expect.That(GraphicsSettings.GetSettingsForRenderPipeline<Pipeline>() == settings && GraphicsSettings.GetSettingsForRenderPipeline(typeof(Pipeline)) == settings, "global settings are found by type");
        GraphicsSettings.UnregisterRenderPipelineSettings<Pipeline>();
        Expect.That(GraphicsSettings.GetSettingsForRenderPipeline(typeof(Pipeline)) == null, "unregistered");
        GraphicsSettings.UpdateGraphicsSettings(settings, typeof(Pipeline));
        Expect.That(GraphicsSettings.GetSettingsForRenderPipeline<Pipeline>() == settings, "UpdateGraphicsSettings registers");
        GraphicsSettings.UnregisterRenderPipelineSettings(typeof(Pipeline));
        Expect.Throws<ArgumentNullException>(() => GraphicsSettings.RegisterRenderPipelineSettings(typeof(Pipeline), null!), "null settings");
    }
}
