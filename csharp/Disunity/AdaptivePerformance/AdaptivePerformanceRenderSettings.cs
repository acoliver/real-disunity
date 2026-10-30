namespace UnityEngine.AdaptivePerformance;

/// <summary>
/// Scales and biases that an adaptive performance provider sets for a render pipeline to read. The engine has no
/// adaptive performance provider, so nothing changes these: every multiplier stays 1, every bias 0 and every skip
/// off, which leaves rendering as the pipeline asset configures it. A script may still set them.
/// </summary>
public static class AdaptivePerformanceRenderSettings
{
    public static float MaxShadowDistanceMultiplier { get; set; } = 1f;
    public static float ShadowCascadeDistanceMultiplier { get; set; } = 1f;
    public static float RenderScaleMultiplier { get; set; } = 1f;
    public static float MainLightShadowmapResolutionMultiplier { get; set; } = 1f;
    public static int MainLightShadowCascadesCountBias { get; set; }
    public static int ShadowQualityBias { get; set; }
    public static float LutBias { get; set; }
    public static int AntiAliasingQualityBias { get; set; }
    public static bool SkipDynamicBatching { get; set; }
    public static bool SkipFrontToBackSorting { get; set; }
    public static bool SkipTransparentObjects { get; set; }
}
