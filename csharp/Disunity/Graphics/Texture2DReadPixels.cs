namespace UnityEngine;

/// <summary>
/// ReadPixels copies a rectangle of <see cref="RenderTexture.active"/> into this texture. The world records the request after the
/// offscreen work that drew the render texture, the host's flush executes that work and answers with the texels the render
/// texture stores (an sRGB texture is not decoded), and the texels go in through the SetPixels path. Reading the screen has no
/// path and throws <see cref="NotSupportedException"/>. As in the engine, the change reaches the GPU copy on <see cref="Apply()"/>.
/// </summary>
public sealed partial class Texture2D
{
    public void ReadPixels(Rect source, int destX, int destY) => ReadPixels(source, destX, destY, true);

    /// <summary>The position of <paramref name="source"/> is floored and its size rounded to whole texels. A rectangle outside the
    /// render texture throws <see cref="ArgumentException"/>; an empty one copies nothing. The part of the rectangle that lands
    /// outside this texture is dropped (the engine's choice). When the host wrote nothing, because no flush handler is registered
    /// or the flush failed, the texels stay as they were. <paramref name="recalculateMipMaps"/> regenerates the smaller levels
    /// from the new base level.</summary>
    public void ReadPixels(Rect source, int destX, int destY, bool recalculateMipMaps)
    {
        RequirePixels(nameof(ReadPixels));
        var from = RenderTexture.active
            ?? throw new NotSupportedException("Texture2D.ReadPixels from the screen is not supported yet: set RenderTexture.active to a render texture");
        var (x, y) = (Mathf.FloorToInt(source.x), Mathf.FloorToInt(source.y));
        var (width, height) = (Mathf.RoundToInt(source.width), Mathf.RoundToInt(source.height));
        if (x < 0 || y < 0 || width < 0 || height < 0 || x + width > from.width || y + height > from.height)
            throw new ArgumentException($"Texture2D.ReadPixels: rectangle {width}x{height} at ({x}, {y}) is outside the {from.width}x{from.height} render texture");
        var (left, bottom) = (Math.Max(destX, 0), Math.Max(destY, 0));
        var (blockWidth, blockHeight) = (Math.Min(destX + width, _width) - left, Math.Min(destY + height, _height) - bottom);
        if (blockWidth <= 0 || blockHeight <= 0) return;
        var request = WorldGraphics.RecordReadPixels(from.WorldId, (uint)(x + left - destX), (uint)(y + bottom - destY), (uint)blockWidth, (uint)blockHeight);
        WorldCompute.Flush();
        var texels = WorldGraphics.TakePixels(request, blockWidth * blockHeight * 4);
        if (texels is null) return;
        var colors = new Color[blockWidth * blockHeight];
        for (var i = 0; i < colors.Length; i++)
            colors[i] = new Color(texels[i * 4], texels[i * 4 + 1], texels[i * 4 + 2], texels[i * 4 + 3]);
        SetPixels(left, bottom, blockWidth, blockHeight, colors, 0);
        if (recalculateMipMaps)
            for (var level = 1; level < _mipCount; level++) Downsample(level);
    }
}
