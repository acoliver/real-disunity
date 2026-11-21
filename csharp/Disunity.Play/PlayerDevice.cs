using System.Text.Json;
using Disunity.Hosting;
using UnityEngine.Experimental.Rendering;

namespace Disunity.Play;

/// <summary>The graphics device a built player reports.</summary>
internal static class PlayerDevice
{
    /// <summary>Usages the offscreen renderer gives every format it lists: it draws into, samples and blends them. It binds
    /// none of them for random writes, so <see cref="GraphicsFormatUsage.LoadStore"/> and the rest are not reported.</summary>
    private const GraphicsFormatUsage Drawn = GraphicsFormatUsage.Render | GraphicsFormatUsage.Sample | GraphicsFormatUsage.Blend;

    /// <summary>The capabilities of a host that samples a depth texture with a comparison sampler and as raw depth.</summary>
    private const GraphicsCapabilities DepthSampling = GraphicsCapabilities.Shadows | GraphicsCapabilities.RawShadowDepthSampling;

    /// <summary><see cref="GraphicsDeviceInfo.Windowless"/>, supporting the request's <c>render_formats</c> for drawing,
    /// sampling and blending, reporting shadows (<see cref="SystemInfo.supportsShadows"/> and
    /// <see cref="SystemInfo.supportsRawShadowDepthSampling"/>) when the request's <c>shadow_sampling</c> is true because the
    /// host reads a depth render texture through a comparison sampler, and declaring
    /// <see cref="GraphicsCapabilities.ReversedZBuffer"/> when the request's <c>reversed_z_buffer</c> is true, so managed code
    /// sees the depth direction the host draws with. A request without the list supports no format, and one without a flag
    /// leaves that capability off.</summary>
    internal static GraphicsDeviceInfo Of(JsonElement request)
    {
        var device = GraphicsDeviceInfo.Windowless;
        if (request.TryGetProperty("render_formats", out var list))
        {
            var formats = list.EnumerateArray().Select(code => (GraphicsFormat)code.GetInt32()).ToHashSet();
            device = device with { FormatSupport = (format, usage) => formats.Contains(format) && (usage & ~Drawn) == 0 && usage != 0 };
        }
        var capabilities = device.Capabilities;
        if (IsTrue(request, "shadow_sampling")) capabilities |= DepthSampling;
        if (IsTrue(request, "reversed_z_buffer")) capabilities |= GraphicsCapabilities.ReversedZBuffer;
        return device with { Capabilities = capabilities };
    }

    private static bool IsTrue(JsonElement request, string name) =>
        request.TryGetProperty(name, out var flag) && flag.ValueKind == JsonValueKind.True;
}
