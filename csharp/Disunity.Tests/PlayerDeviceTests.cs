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
            ShadowSamplingIsReportedWhenTheHostSamplesDepth();
            ReversedDepthIsDeclaredOnlyWhenAsked();
        }
        finally
        {
            DisunityHost.SetGraphicsDevice(null);
        }
        Console.WriteLine("PASS: a player's device supports the request's render formats for drawing, sampling and blending, and nothing else; it reports shadows when the host samples depth with a comparison");
        Console.WriteLine("PASS: a player's device reports a reversed depth buffer only when the request says the host draws one");
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

    private static void ShadowSamplingIsReportedWhenTheHostSamplesDepth()
    {
        DisunityHost.SetGraphicsDevice(GraphicsDeviceInfo.Windowless);
        Expect.That(!SystemInfo.supportsShadows && !SystemInfo.supportsRawShadowDepthSampling, "the windowless device samples no depth");
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"render_formats":[4,5,8,45,48,52]}""")));
        Expect.That(!SystemInfo.supportsShadows, "a host that does not say it samples depth with a comparison reports no shadows");
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"render_formats":[4,5,8,45,48,52],"shadow_sampling":true}""")));
        Expect.That(SystemInfo.supportsShadows && SystemInfo.supportsRawShadowDepthSampling, "a host that samples depth with a comparison reports shadows");
        Expect.That(SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGB32), "and keeps its render formats");
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"shadow_sampling":true}""")));
        Expect.That(SystemInfo.supportsShadows && !SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGB32), "the flag does not need a format list");
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"render_formats":[4,5,8,45,48,52],"shadow_sampling":false}""")));
        Expect.That(!SystemInfo.supportsShadows, "a host that says it cannot reports no shadows");
    }

    private static void ReversedDepthIsDeclaredOnlyWhenAsked()
    {
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"render_formats":[4],"reversed_z_buffer":true}""")));
        Expect.That(SystemInfo.usesReversedZBuffer, "a request that says so declares a reversed depth buffer");
        Expect.That(SystemInfo.SupportsRenderTextureFormat(RenderTextureFormat.ARGB32), "and keeps its formats");
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"reversed_z_buffer":true}""")));
        Expect.That(SystemInfo.usesReversedZBuffer, "a request without formats can declare it too");
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"render_formats":[4],"reversed_z_buffer":false}""")));
        Expect.That(!SystemInfo.usesReversedZBuffer, "false declares none");
        DisunityHost.SetGraphicsDevice(PlayerDevice.Of(Request("""{"render_formats":[4]}""")));
        Expect.That(!SystemInfo.usesReversedZBuffer, "a request that does not say declares none");
    }

    private static JsonElement Request(string json) => JsonDocument.Parse(json).RootElement;
}
