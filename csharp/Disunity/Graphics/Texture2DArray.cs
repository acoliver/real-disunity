using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A stack of equally sized 2D slices whose pixels live in managed memory. Each slice holds its whole mip chain,
/// one slice after another, and a level stores its rows from the bottom up. Formats with a codec (the 8-bit, 16-bit,
/// half and float color formats and the packed 16-bit ones) support pixel access. Compressed formats keep bytes only,
/// and so does a format with no <see cref="TextureFormat"/>, which only a player's plan can create; their pixel
/// members throw <see cref="NotSupportedException"/>. Nothing uploads the pixels to a GPU yet.
/// </summary>
public sealed partial class Texture2DArray : Texture
{
    private const int MaxSize = 16384;
    private const int MaxDepth = 2048;

    private readonly int _width, _height, _depth, _mipCount;
    private readonly TextureFormat? _format;
    private readonly GraphicsFormat _graphics;
    private readonly byte[] _data;
    private readonly long[] _offsets;
    private readonly long _sliceSize;
    private bool _readable = true;

    private Texture2DArray(int width, int height, int depth, TextureFormat? format, GraphicsFormat graphics, int mipCount, long rawSize)
    {
        if (width < 1 || height < 1 || width > MaxSize || height > MaxSize)
            throw new ArgumentException($"Texture2DArray size {width}x{height} is outside 1 to {MaxSize}");
        if (depth < 1 || depth > MaxDepth) throw new ArgumentException($"Texture2DArray depth {depth} is outside 1 to {MaxDepth}");
        if (format is { } known && (TexelFormats.IsCrunched(known) || !TexelFormats.CanAllocate(known)))
            throw new NotSupportedException($"Texture2DArray cannot be created in TextureFormat.{known} yet");
        if (mipCount != GenerateAllMips && mipCount < 1) throw new ArgumentException($"mipCount {mipCount} must be 1 or more, or Texture.GenerateAllMips");
        var full = TexelFormats.FullChain(width, height);
        _mipCount = mipCount == GenerateAllMips ? full : Math.Min(mipCount, full);
        (_width, _height, _depth, _format, _graphics) = (width, height, depth, format, graphics);
        _offsets = format is { } layout ? TexelFormats.ChainOffsets(layout, width, height, _mipCount) : [];
        _sliceSize = _offsets.Length > 0 ? _offsets[^1] : 0;
        var total = format is null ? rawSize : _sliceSize * depth;
        if (total > Array.MaxLength) throw new NotSupportedException($"Texture2DArray of {total} bytes is too large");
        _data = new byte[total];
    }

    public Texture2DArray(int width, int height, int depth, TextureFormat textureFormat, bool mipChain) :
        this(width, height, depth, textureFormat, mipChain, false) { }

    public Texture2DArray(int width, int height, int depth, TextureFormat textureFormat, bool mipChain, bool linear) :
        this(width, height, depth, textureFormat, mipChain ? GenerateAllMips : 1, linear) { }

    public Texture2DArray(int width, int height, int depth, TextureFormat textureFormat, bool mipChain, bool linear, bool createUninitialized) :
        this(width, height, depth, textureFormat, mipChain, linear) { }

    public Texture2DArray(int width, int height, int depth, TextureFormat textureFormat, int mipCount, bool linear) :
        this(width, height, depth, textureFormat, GraphicsFormats.OfTexture(textureFormat, linear), mipCount, 0) { }

    public Texture2DArray(int width, int height, int depth, TextureFormat textureFormat, int mipCount, bool linear, bool createUninitialized) :
        this(width, height, depth, textureFormat, mipCount, linear) { }

    public Texture2DArray(int width, int height, int depth, GraphicsFormat format, TextureCreationFlags flags) :
        this(width, height, depth, format, flags, flags.HasFlag(TextureCreationFlags.MipChain) ? GenerateAllMips : 1) { }

    public Texture2DArray(int width, int height, int depth, GraphicsFormat format, TextureCreationFlags flags, int mipCount) :
        this(width, height, depth, GraphicsFormats.ToTextureFormat(format), format, mipCount, 0) { }

    public Texture2DArray(int width, int height, int depth, DefaultFormat format, TextureCreationFlags flags) :
        this(width, height, depth, GraphicsFormats.DefaultColor(format), flags) { }

    public Texture2DArray(int width, int height, int depth, DefaultFormat format, TextureCreationFlags flags, int mipCount) :
        this(width, height, depth, GraphicsFormats.DefaultColor(format), flags, mipCount) { }

    public override int width => _width;
    public override int height => _height;
    public int depth => _depth;
    public override TextureDimension dimension => TextureDimension.Tex2DArray;
    public override int mipmapCount => _mipCount;
    internal override GraphicsFormat GraphicsFormatCore => _graphics;
    public override bool isReadable => _readable;

    /// <summary>The pixel format. A player's array in a <see cref="GraphicsFormat"/> that has no <see cref="TextureFormat"/>
    /// keeps its bytes and reports its format through <see cref="Texture.graphicsFormat"/> only.</summary>
    public TextureFormat format =>
        _format ?? throw new NotSupportedException($"Texture2DArray.format is not supported yet for GraphicsFormat.{_graphics}");

    /// <summary>The raw bytes of every slice, which a player's pixels are read into.</summary>
    internal Span<byte> PixelBytes => _data;

    internal override Object CloneAsset()
    {
        var copy = new Texture2DArray(_width, _height, _depth, _format, _graphics, _mipCount, _data.Length) { _readable = _readable };
        _data.CopyTo(copy._data, 0);
        copy.CopySamplerFrom(this);
        return Named(copy);
    }
}
