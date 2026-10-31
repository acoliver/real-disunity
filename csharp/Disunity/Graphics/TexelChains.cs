namespace UnityEngine;

/// <summary>Mip chain layout and regeneration shared by the textures that keep their pixels in managed memory.</summary>
internal static partial class TexelFormats
{
    /// <summary>The byte offset of each of <paramref name="levels"/> mip levels of a chain, level 0 first, and then the
    /// chain's total size. A zero width or height has no texels, so every offset is 0.</summary>
    internal static long[] ChainOffsets(TextureFormat format, int width, int height, int levels)
    {
        var offsets = new long[levels + 1];
        if (width == 0 || height == 0) return offsets;
        for (var level = 0; level < levels; level++)
            offsets[level + 1] = offsets[level] + LevelSize(format, Math.Max(1, width >> level), Math.Max(1, height >> level));
        return offsets;
    }

    /// <summary>Fills <paramref name="target"/>, a level of <paramref name="width"/> x <paramref name="height"/>, with the average
    /// of the 2x2 texels of the level above it, <paramref name="source"/>, clamping at that level's edge.</summary>
    internal static void Downsample(TextureFormat format, ReadOnlySpan<byte> source, int sourceWidth, int sourceHeight, Span<byte> target, int width, int height)
    {
        var bytes = BytesPerPixel(format);
        for (var y = 0; y < height; y++)
            for (var x = 0; x < width; x++)
            {
                var sum = Texel(format, source, 2 * x, 2 * y, sourceWidth, sourceHeight) + Texel(format, source, 2 * x + 1, 2 * y, sourceWidth, sourceHeight)
                    + Texel(format, source, 2 * x, 2 * y + 1, sourceWidth, sourceHeight) + Texel(format, source, 2 * x + 1, 2 * y + 1, sourceWidth, sourceHeight);
                Write(format, target.Slice((y * width + x) * bytes, bytes), sum * 0.25f);
            }
    }

    private static Color Texel(TextureFormat format, ReadOnlySpan<byte> level, int x, int y, int width, int height)
    {
        var bytes = BytesPerPixel(format);
        return Read(format, level.Slice((Math.Min(y, height - 1) * width + Math.Min(x, width - 1)) * bytes, bytes));
    }
}
