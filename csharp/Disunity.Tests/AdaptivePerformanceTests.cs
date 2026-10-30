using UnityEngine.AdaptivePerformance;

/// <summary>AdaptivePerformanceRenderSettings: neutral values with no provider, and settable.</summary>
internal static class AdaptivePerformanceTests
{
    public static void Run()
    {
        Expect.That(AdaptivePerformanceRenderSettings.MaxShadowDistanceMultiplier == 1f && AdaptivePerformanceRenderSettings.ShadowCascadeDistanceMultiplier == 1f, "shadow distance multipliers are 1");
        Expect.That(AdaptivePerformanceRenderSettings.RenderScaleMultiplier == 1f && AdaptivePerformanceRenderSettings.MainLightShadowmapResolutionMultiplier == 1f, "scale and shadowmap multipliers are 1");
        Expect.That(AdaptivePerformanceRenderSettings.MainLightShadowCascadesCountBias == 0 && AdaptivePerformanceRenderSettings.ShadowQualityBias == 0, "shadow biases are 0");
        Expect.That(AdaptivePerformanceRenderSettings.LutBias == 0f && AdaptivePerformanceRenderSettings.AntiAliasingQualityBias == 0, "LUT and antialiasing biases are 0");
        Expect.That(!AdaptivePerformanceRenderSettings.SkipDynamicBatching && !AdaptivePerformanceRenderSettings.SkipFrontToBackSorting && !AdaptivePerformanceRenderSettings.SkipTransparentObjects, "nothing is skipped");
        AdaptivePerformanceRenderSettings.SkipTransparentObjects = true;
        AdaptivePerformanceRenderSettings.RenderScaleMultiplier = 0.5f;
        Expect.That(AdaptivePerformanceRenderSettings.SkipTransparentObjects && AdaptivePerformanceRenderSettings.RenderScaleMultiplier == 0.5f, "a script can set them");
        AdaptivePerformanceRenderSettings.SkipTransparentObjects = false;
        AdaptivePerformanceRenderSettings.RenderScaleMultiplier = 1f;
        Console.WriteLine("PASS: AdaptivePerformanceRenderSettings are neutral with no provider and settable");
    }
}
