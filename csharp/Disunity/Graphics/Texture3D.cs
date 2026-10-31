using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A volume of width x height x depth voxels whose pixels live in managed memory. Each mip level halves all three
/// sizes (not below one) and stores its depth slices one after another, and a slice stores its rows from the bottom
/// up. Formats with a codec (the 8-bit, 16-bit, half and float color formats and the packed 16-bit ones) support pixel
/// access. Compressed formats keep bytes only, and their pixel members throw <see cref="NotSupportedException"/>.
/// Nothing uploads the pixels to a GPU yet.
/// </summary>
public sealed partial class Texture3D : Texture
{
    private const int MaxSize = 2048;

    private readonly int _width, _height, _depth, _mipCount;
    private readonly TextureFormat _format;
    private readonly GraphicsFormat _graphics;
    private readonly byte[] _data;
    private readonly long[] _offsets;
    private bool _readable = true;

    private Texture3D(int width, int height, int depth, TextureFormat format, GraphicsFormat graphics, int mipCount)
    {
        if (width < 1 || height < 1 || depth < 1 || width > MaxSize || height > MaxSize || depth > MaxSize)
            throw new ArgumentException($"Texture3D size {width}x{height}x{depth} is outside 1 to {MaxSize} in each dimension");
        if (TexelFormats.IsCrunched(format) || !TexelFormats.CanAllocate(format))
            throw new NotSupportedException($"Texture3D cannot be created in TextureFormat.{format} yet");
        if (mipCount != GenerateAllMips && mipCount < 1) throw new ArgumentException($"mipCount {mipCount} must be 1 or more, or Texture.GenerateAllMips");
        var full = TexelFormats.FullChain(Math.Max(width, height), depth);
        _mipCount = mipCount == GenerateAllMips ? full : Math.Min(mipCount, full);
        (_width, _height, _depth, _format, _graphics) = (width, height, depth, format, graphics);
        _offsets = new long[_mipCount + 1];
        for (var level = 0; level < _mipCount; level++)
            _offsets[level + 1] = _offsets[level] + TexelFormats.LevelSize(format, LevelWidth(level), LevelHeight(level)) * LevelDepth(level);
        if (_offsets[^1] > Array.MaxLength) throw new NotSupportedException($"Texture3D of {_offsets[^1]} bytes is too large");
        _data = new byte[_offsets[^1]];
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
    public TextureFormat format => _format;

    internal override Object CloneAsset()
    {
        var copy = new Texture3D(_width, _height, _depth, _format, _graphics, _mipCount) { _readable = _readable };
        _data.CopyTo(copy._data, 0);
        copy.CopySamplerFrom(this);
        return Named(copy);
    }
}
