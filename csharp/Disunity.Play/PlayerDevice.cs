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
    /// sampling and blending, and reporting shadows (<see cref="SystemInfo.supportsShadows"/> and
    /// <see cref="SystemInfo.supportsRawShadowDepthSampling"/>) when the request's <c>shadow_sampling</c> is true: the host reads a
    /// depth render texture through a comparison sampler. A request without the list supports no format, and one without the
    /// flag leaves shadows off.</summary>
    internal static GraphicsDeviceInfo Of(JsonElement request)
    {
        var device = request.TryGetProperty("shadow_sampling", out var sampling) && sampling.ValueKind == JsonValueKind.True
            ? GraphicsDeviceInfo.Windowless with { Capabilities = GraphicsDeviceInfo.Windowless.Capabilities | DepthSampling }
            : GraphicsDeviceInfo.Windowless;
        if (!request.TryGetProperty("render_formats", out var list)) return device;
        var formats = list.EnumerateArray().Select(code => (GraphicsFormat)code.GetInt32()).ToHashSet();
        return device with
        {
            FormatSupport = (format, usage) => formats.Contains(format) && (usage & ~Drawn) == 0 && usage != 0,
        };
    }
}
