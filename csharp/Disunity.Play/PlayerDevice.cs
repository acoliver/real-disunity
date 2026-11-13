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

    /// <summary><see cref="GraphicsDeviceInfo.Windowless"/>, supporting the request's <c>render_formats</c> for drawing,
    /// sampling and blending. A request without the list supports no format.</summary>
    internal static GraphicsDeviceInfo Of(JsonElement request)
    {
        if (!request.TryGetProperty("render_formats", out var list)) return GraphicsDeviceInfo.Windowless;
        var formats = list.EnumerateArray().Select(code => (GraphicsFormat)code.GetInt32()).ToHashSet();
        return GraphicsDeviceInfo.Windowless with
        {
            FormatSupport = (format, usage) => formats.Contains(format) && (usage & ~Drawn) == 0 && usage != 0,
        };
    }
}
