using System.Text.Json;
using Disunity.Hosting;
using Disunity.Play;
using UnityEngine;
using UnityEngine.Experimental.Rendering;

/// <summary>The device a built player reports: the windowless limits, and the formats the request says the offscreen
/// renderer draws into.</summary>
internal static class PlayerDeviceTests
{
    public static void Run()
    {
        try
        {
            ListedFormatsAreDrawnSampledAndBlended();
            NoListSupportsNoFormat();
        }
        finally
        {
            DisunityHost.SetGraphicsDevice(null);
        }
        Console.WriteLine("PASS: a player's device supports the request's render formats for drawing, sampling and blending, and nothing else");
    }

    private static void ListedFormatsAreDrawnSampledAndBlended()
    {
        var device = PlayerDevice.Of(Request("""{"render_formats":[4,5,8,45,48,52]}"""));
        Expect.Equal(GraphicsDeviceInfo.Windowless.MaxTextureSize, device.MaxTextureSize, "the windowless limits stay");
        DisunityHost.SetGraphicsDevice(device);
        Expect.That(SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGB32), "ARGB32 is drawn into");
        Expect.That(SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGBHalf), "ARGBHalf is drawn into");
        Expect.That(SystemInfo.SupportsBlendingOnRenderTextureFormat(RenderTextureFormat.ARGBHalf), "ARGBHalf is blended");
        Expect.That(SystemInfo.IsFormatSupported(GraphicsFormat.R32G32B32A32_SFloat, GraphicsFormatUsage.Sample), "a listed format is sampled");
        Expect.That(!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.RGHalf), "RGHalf is not listed");
        Expect.That(!SystemInfo.SupportsRandomWriteOnRenderTextureFormat(RenderTextureFormat.ARGBHalf), "no format is reported for random writes");
        Expect.That(!SystemInfo.IsFormatSupported(GraphicsFormat.R8G8B8A8_UNorm, GraphicsFormatUsage.Render | GraphicsFormatUsage.LoadStore), "a usage set that includes LoadStore is refused");
    }

    private static void NoListSupportsNoFormat()
    {
        var device = PlayerDevice.Of(Request("{}"));
        Expect.That(device == GraphicsDeviceInfo.Windowless, "a request without the list is the windowless device");
        DisunityHost.SetGraphicsDevice(device);
        Expect.That(!SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGB32), "and supports no format");
    }

    private static JsonElement Request(string json) => JsonDocument.Parse(json).RootElement;
}
