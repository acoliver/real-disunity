namespace UnityEngine;

/// <summary>Pixel access by array element. Coordinates start at the bottom left of a slice and colors are stored as given,
/// with no sRGB conversion.</summary>
public sealed partial class Texture2DArray
{
    private int LevelWidth(int level) => Math.Max(1, _width >> level);
    private int LevelHeight(int level) => Math.Max(1, _height >> level);

    private Span<byte> LevelBytes(int element, int level) =>
        _data.AsSpan((int)(element * _sliceSize + _offsets[level]), (int)(_offsets[level + 1] - _offsets[level]));

    /// <summary>The format of a readable array with a codec, after checking <paramref name="element"/> and <paramref name="level"/>.</summary>
    private TextureFormat RequirePixels(string member, int element, int level)
    {
        if (!_readable) throw new UnityException($"Texture2DArray.{member}: texture '{name}' is not readable");
        if (_format is not { } known || !TexelFormats.HasCodec(known))
            throw new NotSupportedException($"Texture2DArray.{member} is not supported yet for GraphicsFormat.{_graphics}");
        if (element < 0 || element >= _depth) throw new ArgumentException($"Texture2DArray.{member}: array element {element} is outside 0 to {_depth - 1}");
        if (level < 0 || level >= _mipCount) throw new ArgumentException($"Texture2DArray.{member}: mip level {level} is outside 0 to {_mipCount - 1}");
        return known;
    }

    public Color[] GetPixels(int arrayElement) => GetPixels(arrayElement, 0);

    public Color[] GetPixels(int arrayElement, int miplevel)
    {
        var known = RequirePixels(nameof(GetPixels), arrayElement, miplevel);
        var bytes = TexelFormats.BytesPerPixel(known);
        var level = LevelBytes(arrayElement, miplevel);
        var colors = new Color[LevelWidth(miplevel) * LevelHeight(miplevel)];
        for (var i = 0; i < colors.Length; i++) colors[i] = TexelFormats.Read(known, level.Slice(i * bytes, bytes));
        return colors;
    }

    public Color32[] GetPixels32(int arrayElement) => GetPixels32(arrayElement, 0);

    public Color32[] GetPixels32(int arrayElement, int miplevel) => Array.ConvertAll(GetPixels(arrayElement, miplevel), color => (Color32)color);

    public void SetPixels(Color[] colors, int arrayElement) => SetPixels(colors, arrayElement, 0);

    public void SetPixels(Color[] colors, int arrayElement, int miplevel)
    {
        ArgumentNullException.ThrowIfNull(colors);
        var known = RequirePixels(nameof(SetPixels), arrayElement, miplevel);
        var count = LevelWidth(miplevel) * LevelHeight(miplevel);
        if (colors.Length < count) throw new ArgumentException($"Texture2DArray.SetPixels: {colors.Length} colors given, the level needs {count}");
        var bytes = TexelFormats.BytesPerPixel(known);
        var level = LevelBytes(arrayElement, miplevel);
        for (var i = 0; i < count; i++) TexelFormats.Write(known, level.Slice(i * bytes, bytes), colors[i]);
    }

    public void SetPixels32(Color32[] colors, int arrayElement) => SetPixels32(colors, arrayElement, 0);

    public void SetPixels32(Color32[] colors, int arrayElement, int miplevel)
    {
        ArgumentNullException.ThrowIfNull(colors);
        SetPixels(Array.ConvertAll(colors, color => (Color)color), arrayElement, miplevel);
    }

    /// <summary>Regenerates the smaller mip levels of every slice when asked, counts an update and optionally drops readability.</summary>
    public void Apply(bool updateMipmaps = true, bool makeNoLongerReadable = false)
    {
        if (!_readable) throw new UnityException($"Texture2DArray.{nameof(Apply)}: texture '{name}' is not readable");
        if (updateMipmaps && _format is { } known && TexelFormats.HasCodec(known))
            for (var element = 0; element < _depth; element++)
                for (var level = 1; level < _mipCount; level++)
                    TexelFormats.Downsample(known, LevelBytes(element, level - 1), LevelWidth(level - 1), LevelHeight(level - 1),
                        LevelBytes(element, level), LevelWidth(level), LevelHeight(level));
        if (makeNoLongerReadable) _readable = false;
        IncrementUpdateCount();
    }
}
