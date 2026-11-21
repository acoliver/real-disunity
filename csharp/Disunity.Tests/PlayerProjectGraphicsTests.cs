using System.Text.Json;
using Disunity.Hosting;
using Disunity.Play;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The project color space and light flags a player's start applies before its scripts run.</summary>
internal static class PlayerProjectGraphicsTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ALinearProjectStartsLinear();
            AGammaProjectStartsGamma();
            LightFlagsFollowTheProject();
            ARequestWithoutProjectSettingsChangesNothing();
            AnUnknownColorSpaceFails();
        }
        finally
        {
            GraphicsSettings.lightsUseLinearIntensity = false;
            GraphicsSettings.lightsUseColorTemperature = false;
            DisunityHost.UnloadWorld();
        }
        Console.WriteLine("PASS: a player's start applies the project's color space and light flags");
    }

    private static void ALinearProjectStartsLinear()
    {
        Apply("""{"project":{"color_space":"linear"}}""");
        Expect.Equal(ColorSpace.Linear, QualitySettings.activeColorSpace, "linear project");
        Expect.Equal(ColorSpace.Linear, GraphicsFormats.ActiveColorSpace, "the render formats follow the project");
    }

    private static void AGammaProjectStartsGamma()
    {
        Apply("""{"project":{"color_space":"linear"}}""");
        Apply("""{"project":{"color_space":"gamma"}}""");
        Expect.Equal(ColorSpace.Gamma, QualitySettings.activeColorSpace, "gamma project");
        Expect.Equal(ColorSpace.Gamma, GraphicsFormats.ActiveColorSpace, "the render formats follow a gamma project");
    }

    private static void LightFlagsFollowTheProject()
    {
        Apply("""{"project":{"color_space":"linear","lights_use_linear_intensity":true,"lights_use_color_temperature":true}}""");
        Expect.That(GraphicsSettings.lightsUseLinearIntensity && GraphicsSettings.lightsUseColorTemperature, "both light flags on");
        Apply("""{"project":{"color_space":"linear","lights_use_linear_intensity":false}}""");
        Expect.That(!GraphicsSettings.lightsUseLinearIntensity && !GraphicsSettings.lightsUseColorTemperature, "a flag the project omits is off");
    }

    private static void ARequestWithoutProjectSettingsChangesNothing()
    {
        Apply("""{"project":{"color_space":"linear","lights_use_linear_intensity":true}}""");
        Expect.That(PlayerProjectGraphics.Apply(JsonDocument.Parse("{}").RootElement) is null, "no project, nothing applied");
        Expect.Equal(ColorSpace.Linear, QualitySettings.activeColorSpace, "the color space stays");
        Expect.That(GraphicsSettings.lightsUseLinearIntensity, "the light flag stays");
    }

    private static void AnUnknownColorSpaceFails()
    {
        Expect.Throws<InvalidOperationException>(() => Apply("""{"project":{"color_space":"hdr"}}"""), "an unknown color space");
    }

    private static void Apply(string json) => PlayerProjectGraphics.Apply(JsonDocument.Parse(json).RootElement);
}
