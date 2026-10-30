using UnityEngine.Rendering;

namespace UnityEngine.Experimental.Rendering;

/// <summary>
/// Converts between <see cref="TextureFormat"/>, <see cref="RenderTextureFormat"/> and <see cref="GraphicsFormat"/> and
/// reports depth, stencil, sRGB and compression facts about a format. It reads the engine's format tables, so a
/// format outside them throws <see cref="NotSupportedException"/> naming it.
/// </summary>
public static class GraphicsFormatUtility
{
    public static GraphicsFormat GetGraphicsFormat(TextureFormat format, bool isSRGB) => GraphicsFormats.OfTexture(format, !isSRGB);

    public static GraphicsFormat GetGraphicsFormat(RenderTextureFormat format, bool isSRGB) => GraphicsFormats.ColorFormat(format, isSRGB);

    public static GraphicsFormat GetGraphicsFormat(RenderTextureFormat format, RenderTextureReadWrite readWrite) =>
        GraphicsFormats.ColorFormat(format, GraphicsFormats.UsesSrgb(readWrite));

    public static TextureFormat GetTextureFormat(GraphicsFormat format) => GraphicsFormats.ToTextureFormat(format);

    public static RenderTextureFormat GetRenderTextureFormat(GraphicsFormat format) => GraphicsFormats.RenderFormat(format, false);

    public static GraphicsFormat GetSRGBFormat(GraphicsFormat format) => GraphicsFormats.WithSrgb(format, true);

    public static GraphicsFormat GetLinearFormat(GraphicsFormat format) => GraphicsFormats.WithSrgb(format, false);

    public static bool IsSRGBFormat(GraphicsFormat format) => GraphicsFormats.IsSrgb(format);

    public static bool IsDepthFormat(GraphicsFormat format) => GraphicsFormats.DepthBits(format) > 0;

    public static bool IsStencilFormat(GraphicsFormat format) => GraphicsFormats.HasStencil(format);

    public static uint GetDepthBits(GraphicsFormat format) => (uint)GraphicsFormats.DepthBits(format);

    public static uint GetStencilBits(GraphicsFormat format) => GraphicsFormats.HasStencil(format) ? 8u : 0u;

    public static GraphicsFormat GetDepthStencilFormat(int depthBits) => GraphicsFormats.DepthFormat(depthBits);

    public static bool IsCompressedFormat(TextureFormat format) => TexelFormats.IsCompressed(format);
}
