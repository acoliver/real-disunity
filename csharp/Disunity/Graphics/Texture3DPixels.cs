namespace UnityEngine;

/// <summary>Pixel access by voxel. Coordinates start at the bottom left of the first slice and colors are stored as
/// given, with no sRGB conversion. Whole-level arrays list x first, then y, then z.</summary>
public sealed partial class Texture3D
{
    /// <summary>The format of a volume that has a codec, which <see cref="RequirePixels"/> has checked.</summary>
    private TextureFormat Codec => _format!.Value;

    private Span<byte> Level(int level) => _data.AsSpan((int)_offsets[level], (int)(_offsets[level + 1] - _offsets[level]));

    /// <summary>The format of a readable volume with a codec, after checking that it has the mip level.</summary>
    private TextureFormat RequirePixels(string member, int level)
    {
        if (!_readable) throw new UnityException($"Texture3D.{member}: texture '{name}' is not readable");
        if (_format is not { } known || !TexelFormats.HasCodec(known))
            throw new NotSupportedException($"Texture3D.{member} is not supported yet for GraphicsFormat.{_graphics}");
        if (level < 0 || level >= _mipCount) throw new ArgumentException($"Texture3D.{member}: mip level {level} is outside 0 to {_mipCount - 1}");
        return known;
    }

    private int Voxel(string member, int x, int y, int z, int level)
    {
        if (x < 0 || x >= LevelWidth(level) || y < 0 || y >= LevelHeight(level) || z < 0 || z >= LevelDepth(level))
            throw new ArgumentException($"Texture3D.{member}: voxel ({x}, {y}, {z}) is outside level {level}, which is {LevelWidth(level)}x{LevelHeight(level)}x{LevelDepth(level)}");
        return ((z * LevelHeight(level) + y) * LevelWidth(level) + x) * TexelFormats.BytesPerPixel(Codec);
    }

    public Color GetPixel(int x, int y, int z) => GetPixel(x, y, z, 0);

    public Color GetPixel(int x, int y, int z, int mipLevel)
    {
        RequirePixels(nameof(GetPixel), mipLevel);
        return TexelFormats.Read(Codec, Level(mipLevel).Slice(Voxel(nameof(GetPixel), x, y, z, mipLevel), TexelFormats.BytesPerPixel(Codec)));
    }

    public void SetPixel(int x, int y, int z, Color color) => SetPixel(x, y, z, color, 0);

    public void SetPixel(int x, int y, int z, Color color, int mipLevel)
    {
        RequirePixels(nameof(SetPixel), mipLevel);
        TexelFormats.Write(Codec, Level(mipLevel).Slice(Voxel(nameof(SetPixel), x, y, z, mipLevel), TexelFormats.BytesPerPixel(Codec)), color);
    }

    public Color[] GetPixels() => GetPixels(0);

    public Color[] GetPixels(int miplevel)
    {
        RequirePixels(nameof(GetPixels), miplevel);
        var bytes = TexelFormats.BytesPerPixel(Codec);
        var level = Level(miplevel);
        var colors = new Color[LevelWidth(miplevel) * LevelHeight(miplevel) * LevelDepth(miplevel)];
        for (var i = 0; i < colors.Length; i++) colors[i] = TexelFormats.Read(Codec, level.Slice(i * bytes, bytes));
        return colors;
    }

    public Color32[] GetPixels32() => GetPixels32(0);

    public Color32[] GetPixels32(int miplevel) => Array.ConvertAll(GetPixels(miplevel), color => (Color32)color);

    public void SetPixels(Color[] colors) => SetPixels(colors, 0);

    public void SetPixels(Color[] colors, int miplevel)
    {
        ArgumentNullException.ThrowIfNull(colors);
        RequirePixels(nameof(SetPixels), miplevel);
        var count = LevelWidth(miplevel) * LevelHeight(miplevel) * LevelDepth(miplevel);
        if (colors.Length < count) throw new ArgumentException($"Texture3D.SetPixels: {colors.Length} colors given, the level needs {count}");
        var bytes = TexelFormats.BytesPerPixel(Codec);
        var level = Level(miplevel);
        for (var i = 0; i < count; i++) TexelFormats.Write(Codec, level.Slice(i * bytes, bytes), colors[i]);
    }

    public void SetPixels32(Color32[] colors) => SetPixels32(colors, 0);

    public void SetPixels32(Color32[] colors, int miplevel)
    {
        ArgumentNullException.ThrowIfNull(colors);
        SetPixels(Array.ConvertAll(colors, color => (Color)color), miplevel);
    }

    /// <summary>Regenerates the smaller mip levels when asked, stores the bytes with the host again when the texture has an
    /// identity, counts an update and optionally drops readability.</summary>
    public void Apply() => Apply(true, false);

    public void Apply(bool updateMipmaps) => Apply(updateMipmaps, false);

    public void Apply(bool updateMipmaps, bool makeNoLongerReadable)
    {
        if (!_readable) throw new UnityException($"Texture3D.{nameof(Apply)}: texture '{name}' is not readable");
        if (updateMipmaps && _format is { } laid && TexelFormats.HasCodec(laid))
            for (var level = 1; level < _mipCount; level++) Downsample(level);
        if (StoredIdentity != 0) StoreForRenderer();
        if (makeNoLongerReadable) _readable = false;
        IncrementUpdateCount();
    }

    /// <summary>Fills a level with the average of the up to 2x2x2 voxels of the level above it, clamping at its edges.</summary>
    private void Downsample(int level)
    {
        var bytes = TexelFormats.BytesPerPixel(Codec);
        var above = Level(level - 1);
        var target = Level(level);
        for (var z = 0; z < LevelDepth(level); z++)
            for (var y = 0; y < LevelHeight(level); y++)
                for (var x = 0; x < LevelWidth(level); x++)
                {
                    var sum = new Color(0, 0, 0, 0);
                    for (var i = 0; i < 8; i++)
                        sum += Parent(above, level - 1, 2 * x + (i & 1), 2 * y + ((i >> 1) & 1), 2 * z + (i >> 2));
                    TexelFormats.Write(Codec, target.Slice(((z * LevelHeight(level) + y) * LevelWidth(level) + x) * bytes, bytes), sum * 0.125f);
                }
    }

    private Color Parent(ReadOnlySpan<byte> above, int level, int x, int y, int z)
    {
        var bytes = TexelFormats.BytesPerPixel(Codec);
        x = Math.Min(x, LevelWidth(level) - 1);
        y = Math.Min(y, LevelHeight(level) - 1);
        z = Math.Min(z, LevelDepth(level) - 1);
        return TexelFormats.Read(Codec, above.Slice(((z * LevelHeight(level) + y) * LevelWidth(level) + x) * bytes, bytes));
    }
}
