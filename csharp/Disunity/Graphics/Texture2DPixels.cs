namespace UnityEngine;

/// <summary>
/// Pixel access. Coordinates start at the bottom left. A read outside the level wraps by the texture's
/// wrap modes, and so does a write. Colors are stored as given, with no sRGB conversion.
/// </summary>
public sealed partial class Texture2D
{
    private static int Wrap(int coordinate, int size, TextureWrapMode mode)
    {
        switch (mode)
        {
            case TextureWrapMode.Clamp: return Math.Clamp(coordinate, 0, size - 1);
            case TextureWrapMode.Mirror:
                var folded = ((coordinate % (2 * size)) + 2 * size) % (2 * size);
                return folded < size ? folded : 2 * size - 1 - folded;
            case TextureWrapMode.MirrorOnce:
                return Math.Min(coordinate < 0 ? -coordinate - 1 : coordinate, size - 1);
            default: return ((coordinate % size) + size) % size;
        }
    }

    private Color Fetch(int x, int y, int level)
    {
        var (w, h) = (LevelWidth(level), LevelHeight(level));
        var index = Wrap(y, h, wrapModeV) * w + Wrap(x, w, wrapModeU);
        var bytes = TexelFormats.BytesPerPixel(_format);
        return TexelFormats.Read(_format, LevelBytes(level).Slice(index * bytes, bytes));
    }

    private void Store(int x, int y, int level, Color color)
    {
        var (w, h) = (LevelWidth(level), LevelHeight(level));
        var bytes = TexelFormats.BytesPerPixel(_format);
        var index = Wrap(y, h, wrapModeV) * w + Wrap(x, w, wrapModeU);
        TexelFormats.Write(_format, LevelBytes(level).Slice(index * bytes, bytes), color);
    }

    public Color GetPixel(int x, int y) => GetPixel(x, y, 0);

    public Color GetPixel(int x, int y, int mipLevel)
    {
        RequirePixels(nameof(GetPixel));
        return Fetch(x, y, CheckLevel(mipLevel, nameof(GetPixel)));
    }

    public Color GetPixelBilinear(float u, float v) => GetPixelBilinear(u, v, 0);

    /// <summary>Samples between the four nearest texel centers, wrapping by the texture's wrap modes.</summary>
    public Color GetPixelBilinear(float u, float v, int mipLevel)
    {
        RequirePixels(nameof(GetPixelBilinear));
        var level = CheckLevel(mipLevel, nameof(GetPixelBilinear));
        var fx = u * LevelWidth(level) - 0.5f;
        var fy = v * LevelHeight(level) - 0.5f;
        var (x0, y0) = ((int)MathF.Floor(fx), (int)MathF.Floor(fy));
        var (tx, ty) = (fx - x0, fy - y0);
        var bottom = Color.LerpUnclamped(Fetch(x0, y0, level), Fetch(x0 + 1, y0, level), tx);
        var top = Color.LerpUnclamped(Fetch(x0, y0 + 1, level), Fetch(x0 + 1, y0 + 1, level), tx);
        return Color.LerpUnclamped(bottom, top, ty);
    }

    public void SetPixel(int x, int y, Color color) => SetPixel(x, y, color, 0);

    public void SetPixel(int x, int y, Color color, int mipLevel)
    {
        RequirePixels(nameof(SetPixel));
        Store(x, y, CheckLevel(mipLevel, nameof(SetPixel)), color);
    }

    private (int Level, int Count) CheckBlock(int x, int y, int blockWidth, int blockHeight, int mipLevel, string member)
    {
        RequirePixels(member);
        var level = CheckLevel(mipLevel, member);
        if (x < 0 || y < 0 || blockWidth < 0 || blockHeight < 0 || x + blockWidth > LevelWidth(level) || y + blockHeight > LevelHeight(level))
            throw new ArgumentException($"Texture2D.{member}: block {blockWidth}x{blockHeight} at ({x}, {y}) is outside mip level {level}");
        return (level, blockWidth * blockHeight);
    }

    public Color[] GetPixels() => GetPixels(0);

    public Color[] GetPixels(int miplevel) => GetPixels(0, 0, LevelWidth(CheckLevel(miplevel, nameof(GetPixels))), LevelHeight(miplevel), miplevel);

    public Color[] GetPixels(int x, int y, int blockWidth, int blockHeight) => GetPixels(x, y, blockWidth, blockHeight, 0);

    public Color[] GetPixels(int x, int y, int blockWidth, int blockHeight, int miplevel)
    {
        var (level, count) = CheckBlock(x, y, blockWidth, blockHeight, miplevel, nameof(GetPixels));
        var colors = new Color[count];
        for (var row = 0; row < blockHeight; row++)
            for (var column = 0; column < blockWidth; column++)
                colors[row * blockWidth + column] = Fetch(x + column, y + row, level);
        return colors;
    }

    public Color32[] GetPixels32() => GetPixels32(0);

    public Color32[] GetPixels32(int miplevel) => Array.ConvertAll(GetPixels(miplevel), color => (Color32)color);

    public void SetPixels(Color[] colors) => SetPixels(colors, 0);

    public void SetPixels(Color[] colors, int miplevel) =>
        SetPixels(0, 0, LevelWidth(CheckLevel(miplevel, nameof(SetPixels))), LevelHeight(miplevel), colors, miplevel);

    public void SetPixels(int x, int y, int blockWidth, int blockHeight, Color[] colors) => SetPixels(x, y, blockWidth, blockHeight, colors, 0);

    public void SetPixels(int x, int y, int blockWidth, int blockHeight, Color[] colors, int miplevel)
    {
        ArgumentNullException.ThrowIfNull(colors);
        var (level, count) = CheckBlock(x, y, blockWidth, blockHeight, miplevel, nameof(SetPixels));
        if (colors.Length < count) throw new ArgumentException($"Texture2D.SetPixels: {colors.Length} colors given, the block needs {count}");
        for (var row = 0; row < blockHeight; row++)
            for (var column = 0; column < blockWidth; column++)
                Store(x + column, y + row, level, colors[row * blockWidth + column]);
    }

    public void SetPixels32(Color32[] colors) => SetPixels32(colors, 0);

    public void SetPixels32(Color32[] colors, int miplevel)
    {
        ArgumentNullException.ThrowIfNull(colors);
        SetPixels(Array.ConvertAll(colors, color => (Color)color), miplevel);
    }

    public void SetPixels32(int x, int y, int blockWidth, int blockHeight, Color32[] colors) => SetPixels32(x, y, blockWidth, blockHeight, colors, 0);

    public void SetPixels32(int x, int y, int blockWidth, int blockHeight, Color32[] colors, int miplevel)
    {
        ArgumentNullException.ThrowIfNull(colors);
        SetPixels(x, y, blockWidth, blockHeight, Array.ConvertAll(colors, color => (Color)color), miplevel);
    }

    public void Apply() => Apply(true, false);

    public void Apply(bool updateMipmaps) => Apply(updateMipmaps, false);

    /// <summary>Regenerates the smaller mip levels when asked, counts an update and optionally drops readability.</summary>
    public void Apply(bool updateMipmaps, bool makeNoLongerReadable)
    {
        RequireReadable(nameof(Apply));
        if (updateMipmaps && TexelFormats.HasCodec(_format))
            for (var level = 1; level < _mipCount; level++) Downsample(level);
        if (makeNoLongerReadable) _readable = false;
        IncrementUpdateCount();
    }

    /// <summary>Each texel of a level is the average of the 2x2 texels of the level above it.</summary>
    private void Downsample(int level)
    {
        var (w, h, sourceW, sourceH) = (LevelWidth(level), LevelHeight(level), LevelWidth(level - 1), LevelHeight(level - 1));
        var bytes = TexelFormats.BytesPerPixel(_format);
        var source = LevelBytes(level - 1);
        var target = LevelBytes(level);
        for (var y = 0; y < h; y++)
            for (var x = 0; x < w; x++)
            {
                var sum = Sample(source, 2 * x, 2 * y, sourceW, sourceH) + Sample(source, 2 * x + 1, 2 * y, sourceW, sourceH)
                    + Sample(source, 2 * x, 2 * y + 1, sourceW, sourceH) + Sample(source, 2 * x + 1, 2 * y + 1, sourceW, sourceH);
                TexelFormats.Write(_format, target.Slice((y * w + x) * bytes, bytes), sum * 0.25f);
            }
    }

    /// <summary>The texel at (x, y) of a level of the given size, clamped to its edge.</summary>
    private Color Sample(ReadOnlySpan<byte> level, int x, int y, int w, int h)
    {
        var bytes = TexelFormats.BytesPerPixel(_format);
        return TexelFormats.Read(_format, level.Slice((Math.Min(y, h - 1) * w + Math.Min(x, w - 1)) * bytes, bytes));
    }

    /// <summary>Texture2D.Compress, PackTextures, GenerateAtlas, ReadPixels, CopyPixels and the external
    /// texture members need a GPU or a texture compressor, which the engine does not have yet.</summary>
    public void Compress(bool highQuality) => throw Unsupported(nameof(Compress));

    public Rect[] PackTextures(Texture2D[] textures, int padding, int maximumAtlasSize, bool makeNoLongerReadable) => throw Unsupported(nameof(PackTextures));

    public Rect[] PackTextures(Texture2D[] textures, int padding, int maximumAtlasSize) => throw Unsupported(nameof(PackTextures));

    public Rect[] PackTextures(Texture2D[] textures, int padding) => throw Unsupported(nameof(PackTextures));

    public static bool GenerateAtlas(Vector2[] sizes, int padding, int atlasSize, List<Rect> results) =>
        throw new NotSupportedException("Texture2D.GenerateAtlas is not supported yet");

    public void ReadPixels(Rect source, int destX, int destY, bool recalculateMipMaps) => throw Unsupported(nameof(ReadPixels));

    public void ReadPixels(Rect source, int destX, int destY) => throw Unsupported(nameof(ReadPixels));

    public void CopyPixels(Texture src) => throw Unsupported(nameof(CopyPixels));

    public void CopyPixels(Texture src, int srcElement, int srcMip, int dstMip) => throw Unsupported(nameof(CopyPixels));

    public void CopyPixels(Texture src, int srcElement, int srcMip, int srcX, int srcY, int srcWidth, int srcHeight, int dstMip, int dstX, int dstY) =>
        throw Unsupported(nameof(CopyPixels));

    public static Texture2D CreateExternalTexture(int width, int height, TextureFormat format, bool mipChain, bool linear, IntPtr nativeTex) =>
        throw new NotSupportedException("Texture2D.CreateExternalTexture is not supported yet");

    public void UpdateExternalTexture(IntPtr nativeTex) => throw Unsupported(nameof(UpdateExternalTexture));

    private static NotSupportedException Unsupported(string member) => new($"Texture2D.{member} is not supported yet");
}
