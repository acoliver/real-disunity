using Disunity.Hosting;
using UnityEngine;

/// <summary>QualitySettings defaults, validation, level switching and the reset when a world loads.</summary>
internal static class QualitySettingsTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            RoundTrips();
            Validation();
            Levels();
            HostHooks();
        }
        finally { DisunityHost.UnloadWorld(); }
        ResetsWhenAWorldLoads();
        Console.WriteLine("PASS: QualitySettings defaults, validation, quality levels, host hooks and reset on world load");
    }

    private static void Defaults()
    {
        Expect.Equal(0, QualitySettings.antiAliasing, "antiAliasing");
        Expect.Equal(0, QualitySettings.globalTextureMipmapLimit, "globalTextureMipmapLimit");
        Expect.Equal(0, QualitySettings.maximumLODLevel, "maximumLODLevel");
        Expect.Equal(2, QualitySettings.maxQueuedFrames, "maxQueuedFrames");
        Expect.Equal(1, QualitySettings.vSyncCount, "vSyncCount");
        Expect.Equal(1f, QualitySettings.lodBias, "lodBias");
        Expect.Equal(150f, QualitySettings.shadowDistance, "shadowDistance");
        Expect.Equal(ShadowQuality.All, QualitySettings.shadows, "shadows");
        Expect.Equal(ColorSpace.Gamma, QualitySettings.activeColorSpace, "activeColorSpace");
        Expect.Equal(0, QualitySettings.GetQualityLevel(), "GetQualityLevel");
        Expect.Sequence(new[] { "Default" }, QualitySettings.names, "names");
    }

    private static void RoundTrips()
    {
        QualitySettings.antiAliasing = 4;
        QualitySettings.globalTextureMipmapLimit = 2;
        QualitySettings.maximumLODLevel = 1;
        QualitySettings.maxQueuedFrames = 3;
        QualitySettings.vSyncCount = 0;
        QualitySettings.lodBias = 2.5f;
        QualitySettings.shadowDistance = 40f;
        QualitySettings.shadows = ShadowQuality.HardOnly;
        Expect.Equal(4, QualitySettings.antiAliasing, "antiAliasing");
        Expect.Equal(2, QualitySettings.globalTextureMipmapLimit, "globalTextureMipmapLimit");
        Expect.Equal(1, QualitySettings.maximumLODLevel, "maximumLODLevel");
        Expect.Equal(3, QualitySettings.maxQueuedFrames, "maxQueuedFrames");
        Expect.Equal(0, QualitySettings.vSyncCount, "vSyncCount");
        Expect.Equal(2.5f, QualitySettings.lodBias, "lodBias");
        Expect.Equal(40f, QualitySettings.shadowDistance, "shadowDistance");
        Expect.Equal(ShadowQuality.HardOnly, QualitySettings.shadows, "shadows");
    }

    private static void Validation()
    {
        foreach (var good in new[] { 0, 2, 4, 8 })
        {
            QualitySettings.antiAliasing = good;
            Expect.Equal(good, QualitySettings.antiAliasing, "antiAliasing accepts " + good);
        }
        foreach (var bad in new[] { -1, 1, 3, 5, 16 })
            Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.antiAliasing = bad, "antiAliasing " + bad);
        Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.globalTextureMipmapLimit = -1, "negative mipmap limit");
        Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.maximumLODLevel = -1, "negative LOD level");
        Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.shadowDistance = -0.5f, "negative shadow distance");
        Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.shadowDistance = float.NaN, "NaN shadow distance");
        QualitySettings.shadowDistance = 0;
        foreach (var good in new[] { 0, 1, 4 }) QualitySettings.vSyncCount = good;
        foreach (var bad in new[] { -1, 5 })
            Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.vSyncCount = bad, "vSyncCount " + bad);
        foreach (var good in new[] { 1, 10 }) QualitySettings.maxQueuedFrames = good;
        foreach (var bad in new[] { 0, 11 })
            Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.maxQueuedFrames = bad, "maxQueuedFrames " + bad);
        foreach (var bad in new[] { 0f, -1f, float.NaN, float.PositiveInfinity })
            Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.lodBias = bad, "lodBias " + bad);
        Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.shadows = (ShadowQuality)9, "undefined shadow quality");
    }

    private static void Levels()
    {
        DisunityHost.SetQualityLevels(["Low", "High"], 1);
        Expect.Sequence(new[] { "Low", "High" }, QualitySettings.names, "names");
        Expect.Equal(1, QualitySettings.GetQualityLevel(), "current level");
        QualitySettings.names[0] = "Changed";
        Expect.Equal("Low", QualitySettings.names[0], "names returns a copy");
        QualitySettings.lodBias = 3f;
        QualitySettings.SetQualityLevel(0);
        Expect.Equal(0, QualitySettings.GetQualityLevel(), "SetQualityLevel");
        Expect.Equal(3f, QualitySettings.lodBias, "switching levels keeps the stored values");
        QualitySettings.SetQualityLevel(1, true);
        Expect.Equal(1, QualitySettings.GetQualityLevel(), "SetQualityLevel with expensive changes");
        Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.SetQualityLevel(2), "level 2 of 2");
        Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.SetQualityLevel(-1, false), "level -1");
        Expect.Equal(1, QualitySettings.GetQualityLevel(), "a rejected level changes nothing");
    }

    private static void HostHooks()
    {
        Expect.Throws<ArgumentException>(() => DisunityHost.SetQualityLevels([], 0), "no levels");
        Expect.Throws<ArgumentOutOfRangeException>(() => DisunityHost.SetQualityLevels(["A"], 1), "current past the end");
        Expect.Throws<ArgumentOutOfRangeException>(() => DisunityHost.SetQualityLevels(["A"], -1), "negative current");
        Expect.Sequence(new[] { "Low", "High" }, QualitySettings.names, "a rejected call changes nothing");
        DisunityHost.SetColorSpace(ColorSpace.Linear);
        Expect.Equal(ColorSpace.Linear, QualitySettings.activeColorSpace, "SetColorSpace");
    }

    private static void ResetsWhenAWorldLoads()
    {
        QualitySettings.antiAliasing = 8;
        QualitySettings.vSyncCount = 4;
        QualitySettings.shadowDistance = 5f;
        DisunityHost.SetColorSpace(ColorSpace.Linear);
        DisunityHost.SetQualityLevels(["Low", "High"], 1);
        DisunityHost.LoadWorld();
        try
        {
            Expect.Equal(0, QualitySettings.antiAliasing, "antiAliasing reset");
            Expect.Equal(1, QualitySettings.vSyncCount, "vSyncCount reset");
            Expect.Equal(150f, QualitySettings.shadowDistance, "shadowDistance reset");
            Expect.Equal(ColorSpace.Gamma, QualitySettings.activeColorSpace, "color space reset");
            Expect.Sequence(new[] { "Default" }, QualitySettings.names, "levels reset");
            Expect.Equal(0, QualitySettings.GetQualityLevel(), "level reset");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
