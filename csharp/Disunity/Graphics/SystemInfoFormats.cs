using Disunity.Hosting;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The format queries of <see cref="SystemInfo"/>, answered from the registered graphics device's format
/// predicate; the null device supports no format and no multisampling. A format the engine has no mapping for
/// is unsupported, not an error.
/// </summary>
public sealed partial class SystemInfo
{
    public static bool IsFormatSupported(GraphicsFormat format, GraphicsFormatUsage usage) =>
        Device.FormatSupport?.Invoke(format, usage) ?? false;

    /// <summary>FormatUsage values are the bit positions of GraphicsFormatUsage flags.</summary>
    public static bool IsFormatSupported(GraphicsFormat format, FormatUsage usage) => IsFormatSupported(format, ToFlag(usage));

    /// <summary>The format itself when the device supports it for the usage, else GraphicsFormat.None: the engine does not search for a substitute.</summary>
    public static GraphicsFormat GetCompatibleFormat(GraphicsFormat format, GraphicsFormatUsage usage) =>
        IsFormatSupported(format, usage) ? format : GraphicsFormat.None;

    public static GraphicsFormat GetCompatibleFormat(GraphicsFormat format, FormatUsage usage) => GetCompatibleFormat(format, ToFlag(usage));

    /// <summary>
    /// Unity's documented defaults: LDR is 8-bit RGBA, sRGB in a linear project, HDR is RGBA half float, DepthStencil
    /// is 24-bit depth with 8-bit stencil and Shadow is 32-bit float depth. They do not depend on the device.
    /// DefaultFormat.Video has no mapping and throws NotSupportedException.
    /// </summary>
    public static GraphicsFormat GetGraphicsFormat(DefaultFormat format) => format switch
    {
        DefaultFormat.DepthStencil => GraphicsFormat.D24_UNorm_S8_UInt,
        DefaultFormat.Shadow => GraphicsFormat.D32_SFloat,
        _ => GraphicsFormats.DefaultColor(format),
    };

    public static bool SupportsRenderTextureFormat(RenderTextureFormat format) => SupportsRenderFormat(format, GraphicsFormatUsage.Render);

    public static bool SupportsBlendingOnRenderTextureFormat(RenderTextureFormat format) => SupportsRenderFormat(format, GraphicsFormatUsage.Blend);

    public static bool SupportsRandomWriteOnRenderTextureFormat(RenderTextureFormat format) => SupportsRenderFormat(format, GraphicsFormatUsage.LoadStore);

    public static bool SupportsTypedUAVShaderLoadStoreOnGraphicsFormat(GraphicsFormat graphicsFormat, bool isLoad) =>
        IsFormatSupported(graphicsFormat, GraphicsFormatUsage.LoadStore);

    public static bool SupportsTextureFormat(TextureFormat format)
    {
        try { return IsFormatSupported(GraphicsFormats.OfTexture(format, linear: false), GraphicsFormatUsage.Sample); }
        catch (NotSupportedException) { return false; }
    }

    public static bool SupportsVertexAttributeFormat(VertexAttributeFormat format, int dimension) =>
        Device.VertexAttributeSupport?.Invoke(format, dimension) ?? false;

    /// <summary>The largest sample count of 1, 2, 4 or 8 that is at most the request and the device's maximum; 1 means no multisampling.</summary>
    public static int GetRenderTextureSupportedMSAASampleCount(RenderTextureDescriptor desc)
    {
        var limit = Math.Min(desc.msaaSamples, Device.MaxMsaaSamples);
        foreach (var count in new[] { 8, 4, 2 })
            if (limit >= count) return count;
        return 1;
    }

    /// <summary>0 on a device that is not a tiled GPU. For a tiled device the per-format sizes are not known to the engine, so the call throws NotSupportedException.</summary>
    public static int GetTiledRenderTargetStorageSize(GraphicsFormat format, int sampleCount) =>
        Has(GraphicsCapabilities.TiledGPU)
            ? throw new NotSupportedException("SystemInfo.GetTiledRenderTargetStorageSize is not supported yet for a tiled GPU: the engine has no per-format storage sizes")
            : 0;

    private static GraphicsFormatUsage ToFlag(FormatUsage usage) => (GraphicsFormatUsage)(1 << (int)usage);

    private static bool SupportsRenderFormat(RenderTextureFormat format, GraphicsFormatUsage usage)
    {
        try { return IsFormatSupported(RenderFormat(format), usage); }
        catch (NotSupportedException) { return false; }
    }

    private static GraphicsFormat RenderFormat(RenderTextureFormat format) => format switch
    {
        RenderTextureFormat.Depth => GraphicsFormat.D24_UNorm_S8_UInt,
        RenderTextureFormat.Shadowmap => GraphicsFormat.D32_SFloat,
        _ => GraphicsFormats.ColorFormat(format, GraphicsFormats.ActiveColorSpace == ColorSpace.Linear),
    };
}
