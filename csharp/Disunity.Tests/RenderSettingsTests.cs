using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>RenderSettings holds the scene's fog, ambient light, reflection and skybox values, starting from a new scene's.</summary>
internal static class RenderSettingsTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            DefaultsAreANewScenes();
            ValuesAreStored();
            RangesAreClamped();
            AWorldReloadRestoresTheDefaults();
            Console.WriteLine("PASS: RenderSettings starts from a new scene's fog, ambient and reflection values, stores writes, clamps ranges and resets with the world");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    private static void DefaultsAreANewScenes()
    {
        Expect.That(!RenderSettings.fog, "fog is off");
        Expect.Equal(FogMode.ExponentialSquared, RenderSettings.fogMode, "fog mode");
        Expect.Near(0.01f, RenderSettings.fogDensity, "fog density");
        Expect.Near(0f, RenderSettings.fogStartDistance, "fog start");
        Expect.Near(300f, RenderSettings.fogEndDistance, "fog end");
        Expect.Equal(new Color(0.5f, 0.5f, 0.5f, 1f), RenderSettings.fogColor, "fog color");
        Expect.Equal(AmbientMode.Skybox, RenderSettings.ambientMode, "ambient mode");
        Expect.Near(1f, RenderSettings.ambientIntensity, "ambient intensity");
        Expect.Near(1f, RenderSettings.reflectionIntensity, "reflection intensity");
        Expect.Equal(1, RenderSettings.reflectionBounces, "reflection bounces");
        Expect.Equal(DefaultReflectionMode.Skybox, RenderSettings.defaultReflectionMode, "reflection mode");
        Expect.Equal(128, RenderSettings.defaultReflectionResolution, "reflection resolution");
        Expect.That(RenderSettings.skybox == null && RenderSettings.sun == null && RenderSettings.customReflectionTexture == null, "no skybox, sun or custom reflection");
        Expect.That(RenderSettings.ambientProbe == default, "an empty ambient probe");
    }

    private static void ValuesAreStored()
    {
        RenderSettings.fog = true;
        RenderSettings.fogMode = FogMode.Linear;
        RenderSettings.fogColor = Color.red;
        RenderSettings.fogStartDistance = 5f;
        RenderSettings.fogEndDistance = 50f;
        RenderSettings.fogDensity = 0.5f;
        RenderSettings.ambientMode = AmbientMode.Trilight;
        RenderSettings.ambientSkyColor = Color.blue;
        RenderSettings.ambientEquatorColor = Color.green;
        RenderSettings.ambientGroundColor = Color.black;
        RenderSettings.ambientLight = Color.white;
        RenderSettings.subtractiveShadowColor = Color.gray;
        RenderSettings.haloStrength = 0.25f;
        RenderSettings.flareStrength = 2f;
        RenderSettings.flareFadeSpeed = 4f;
        Expect.That(RenderSettings.fog && RenderSettings.fogMode == FogMode.Linear && RenderSettings.fogColor == Color.red, "fog reads back");
        Expect.That(RenderSettings.fogStartDistance == 5f && RenderSettings.fogEndDistance == 50f && RenderSettings.fogDensity == 0.5f, "fog range reads back");
        Expect.That(RenderSettings.ambientMode == AmbientMode.Trilight && RenderSettings.ambientSkyColor == Color.blue, "ambient mode and sky read back");
        Expect.That(RenderSettings.ambientEquatorColor == Color.green && RenderSettings.ambientGroundColor == Color.black, "ambient equator and ground read back");
        Expect.That(RenderSettings.ambientLight == Color.white && RenderSettings.subtractiveShadowColor == Color.gray, "ambient light and shadow color read back");
        Expect.That(RenderSettings.haloStrength == 0.25f && RenderSettings.flareStrength == 2f && RenderSettings.flareFadeSpeed == 4f, "flare values read back");
        var probe = new SphericalHarmonicsL2();
        probe[0, 0] = 0.5f;
        RenderSettings.ambientProbe = probe;
        Expect.That(RenderSettings.ambientProbe == probe, "the ambient probe reads back");
        var sun = DisunityHost.CreateGameObject().AddComponent<Light>();
        var material = new Material(DisunityHost.RegisterShader("Skybox/Test"));
        RenderSettings.sun = sun;
        RenderSettings.skybox = material;
        Expect.That(RenderSettings.sun == sun && RenderSettings.skybox == material, "the sun and skybox read back");
        RenderSettings.defaultReflectionMode = DefaultReflectionMode.Custom;
        RenderSettings.defaultReflectionResolution = 256;
        Expect.That(RenderSettings.defaultReflectionMode == DefaultReflectionMode.Custom && RenderSettings.defaultReflectionResolution == 256, "reflection settings read back");
    }

    private static void RangesAreClamped()
    {
        RenderSettings.ambientIntensity = 20f;
        Expect.Near(8f, RenderSettings.ambientIntensity, "ambient intensity stops at 8");
        RenderSettings.ambientIntensity = -1f;
        Expect.Near(0f, RenderSettings.ambientIntensity, "and at 0");
        RenderSettings.reflectionIntensity = 3f;
        Expect.Near(1f, RenderSettings.reflectionIntensity, "reflection intensity stops at 1");
        RenderSettings.reflectionBounces = 9;
        Expect.Equal(5, RenderSettings.reflectionBounces, "reflection bounces stop at 5");
        RenderSettings.reflectionBounces = 0;
        Expect.Equal(1, RenderSettings.reflectionBounces, "and start at 1");
    }

    private static void AWorldReloadRestoresTheDefaults()
    {
        DisunityHost.UnloadWorld();
        DisunityHost.LoadWorld();
        Expect.That(!RenderSettings.fog && RenderSettings.fogMode == FogMode.ExponentialSquared && RenderSettings.skybox == null, "fog and skybox are back to the defaults");
        Expect.That(RenderSettings.ambientMode == AmbientMode.Skybox && RenderSettings.ambientIntensity == 1f && RenderSettings.sun == null, "ambient values are back to the defaults");
    }
}
