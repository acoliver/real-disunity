using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A volume of width x height x depth voxels whose pixels live in managed memory. Each mip level halves all three
/// sizes (not below one) and stores its depth slices one after another, and a slice stores its rows from the bottom
/// up. Formats with a codec (the 8-bit, 16-bit, half and float color formats and the packed 16-bit ones) support pixel
/// access. Compressed formats keep bytes only, and so does a format with no <see cref="TextureFormat"/>, which only a
/// player's plan can create; their pixel members throw <see cref="NotSupportedException"/>. A material or a property block
/// that holds the volume sends it to the host as an asset texture under a renderer identity (see
/// <see cref="RendererIdentity"/>), and the host draws from the bytes stored under it, in the colour space of its
/// <see cref="Texture.graphicsFormat"/>; <see cref="Apply(bool, bool)"/> stores them again. A volume the host cannot store
/// (no bytes, or more than it stores) is sent as no texture.
/// </summary>
public sealed partial class Texture3D : Texture
{
    private const int MaxSize = 2048;

    private readonly int _width, _height, _depth, _mipCount;
    private readonly TextureFormat? _format;
    private readonly GraphicsFormat _graphics;
    private readonly byte[] _data;
    private readonly long[] _offsets;
    private bool _readable = true;

    private Texture3D(int width, int height, int depth, TextureFormat? format, GraphicsFormat graphics, int mipCount, long rawSize = 0)
    {
        if (width < 1 || height < 1 || depth < 1 || width > MaxSize || height > MaxSize || depth > MaxSize)
            throw new ArgumentException($"Texture3D size {width}x{height}x{depth} is outside 1 to {MaxSize} in each dimension");
        if (format is { } known && (TexelFormats.IsCrunched(known) || !TexelFormats.CanAllocate(known)))
            throw new NotSupportedException($"Texture3D cannot be created in TextureFormat.{known} yet");
        if (mipCount != GenerateAllMips && mipCount < 1) throw new ArgumentException($"mipCount {mipCount} must be 1 or more, or Texture.GenerateAllMips");
        var full = TexelFormats.FullChain(Math.Max(width, height), depth);
        _mipCount = mipCount == GenerateAllMips ? full : Math.Min(mipCount, full);
        (_width, _height, _depth, _format, _graphics) = (width, height, depth, format, graphics);
        _offsets = new long[_mipCount + 1];
        if (format is { } laid)
            for (var level = 0; level < _mipCount; level++)
                _offsets[level + 1] = _offsets[level] + TexelFormats.LevelSize(laid, LevelWidth(level), LevelHeight(level)) * LevelDepth(level);
        var total = format is null ? rawSize : _offsets[^1];
        if (total > Array.MaxLength) throw new NotSupportedException($"Texture3D of {total} bytes is too large");
        _data = new byte[total];
    }

    public Texture3D(int width, int height, int depth, TextureFormat textureFormat, bool mipChain) :
        this(width, height, depth, textureFormat, mipChain ? GenerateAllMips : 1) { }

    public Texture3D(int width, int height, int depth, TextureFormat textureFormat, bool mipChain, bool createUninitialized) :
        this(width, height, depth, textureFormat, mipChain) { }

    public Texture3D(int width, int height, int depth, TextureFormat textureFormat, int mipCount) :
        this(width, height, depth, textureFormat, GraphicsFormats.OfTexture(textureFormat, false), mipCount) { }

    public Texture3D(int width, int height, int depth, TextureFormat textureFormat, int mipCount, bool createUninitialized) :
        this(width, height, depth, textureFormat, mipCount) { }

    public Texture3D(int width, int height, int depth, GraphicsFormat format, TextureCreationFlags flags) :
        this(width, height, depth, format, flags, flags.HasFlag(TextureCreationFlags.MipChain) ? GenerateAllMips : 1) { }

    public Texture3D(int width, int height, int depth, GraphicsFormat format, TextureCreationFlags flags, int mipCount) :
        this(width, height, depth, GraphicsFormats.ToTextureFormat(format), format, mipCount) { }

    public Texture3D(int width, int height, int depth, DefaultFormat format, TextureCreationFlags flags) :
        this(width, height, depth, GraphicsFormats.DefaultColor(format), flags) { }

    public Texture3D(int width, int height, int depth, DefaultFormat format, TextureCreationFlags flags, int mipCount) :
        this(width, height, depth, GraphicsFormats.DefaultColor(format), flags, mipCount) { }

    private int LevelWidth(int level) => Math.Max(1, _width >> level);
    private int LevelHeight(int level) => Math.Max(1, _height >> level);
    private int LevelDepth(int level) => Math.Max(1, _depth >> level);

    public override int width => _width;
    public override int height => _height;
    public int depth => _depth;
    public override TextureDimension dimension => TextureDimension.Tex3D;
    public override int mipmapCount => _mipCount;
    internal override GraphicsFormat GraphicsFormatCore => _graphics;
    public override bool isReadable => _readable;

    /// <summary>The pixel format. A player's volume in a <see cref="GraphicsFormat"/> that has no <see cref="TextureFormat"/>
    /// keeps its bytes and reports its format through <see cref="Texture.graphicsFormat"/> only.</summary>
    public TextureFormat format =>
        _format ?? throw new NotSupportedException($"Texture3D.format is not supported yet for GraphicsFormat.{_graphics}");

    internal override Object CloneAsset()
    {
        var copy = new Texture3D(_width, _height, _depth, _format, _graphics, _mipCount, _data.Length) { _readable = _readable };
        _data.CopyTo(copy._data, 0);
        copy.CopySamplerFrom(this);
        return Named(copy);
    }
}
