using UnityEngine;
using UnityEngine.Experimental.Rendering;

/// <summary>The engine has one display with no HDR output; HDROutputSettings says so and refuses to change it.</summary>
internal static class HdrOutputTests
{
    public static void Run()
    {
        var main = HDROutputSettings.main!;
        Expect.That(ReferenceEquals(main, HDROutputSettings.main), "main is the one display's settings");
        Expect.Equal(1, HDROutputSettings.displays.Length, "one display");
        Expect.That(ReferenceEquals(HDROutputSettings.displays[0], main), "the display array holds main");
        Expect.That(!main.available && !main.active, "no HDR output is available or active");
        main.RequestHDRModeChange(true);
        Expect.That(!main.active && !main.HDRModeChangeRequested, "a request to turn HDR on changes nothing, the engine's choice");
        Expect.Equal(ColorGamut.sRGB, main.displayColorGamut, "the display gamut is sRGB");
        Expect.Equal(GraphicsFormat.R8G8B8A8_UNorm, main.graphicsFormat, "the output format is 8-bit unsigned normalized RGBA");
        Expect.That(main.format == main.graphicsFormat, "format and graphicsFormat agree");
        Expect.That(main.automaticHDRTonemapping, "automatic tonemapping is on until a pipeline turns it off");
        main.automaticHDRTonemapping = false;
        Expect.That(!main.automaticHDRTonemapping, "automatic tonemapping is stored");
        main.automaticHDRTonemapping = true;
        main.paperWhiteNits = 200;
        Expect.Equal(200, main.paperWhiteNits, "paper white is stored");
        Expect.Equal(0f, main.maxFullFrameToneMapLuminance, "no luminance range is reported");
        Expect.Equal(0f, main.maxToneMapLuminance, "no peak luminance is reported");
        Expect.Equal(0f, main.minToneMapLuminance, "no minimum luminance is reported");
        Console.WriteLine("PASS: HDROutputSettings reports one display without HDR output and ignores a request to turn it on");
    }
}
