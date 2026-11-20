using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A texture of six square faces. The engine holds the size, the format, the mip count and the bytes of the faces, each face
/// with its whole mip chain one after another in the order +X, -X, +Y, -Y, +Z, -Z (a level stores its rows from the bottom up),
/// for a format it can lay out. A material or a property block that holds the cubemap sends it to the host as an asset
/// texture under a renderer identity (see <see cref="RendererIdentity"/>), and the host draws from the bytes stored under it,
/// in the colour space of its <see cref="Texture.graphicsFormat"/>. A cubemap whose bytes the host cannot store (a format with
/// no layout, which has no bytes unless a player's plan gives the cubemap its raw size, or more than the host stores) is sent as no texture. Every pixel member throws
/// <see cref="NotSupportedException"/> naming it. The mip count is clamped to the full chain of the size.
/// </summary>
public partial class Cubemap : Texture
{
    private const int MaxSize = 16384;
    private const int Faces = 6;

    private readonly int _size, _mipCount;
    private readonly GraphicsFormat _graphics;
    private readonly TextureFormat? _format;
    private readonly byte[] _data;
    private readonly long _faceSize;

    private Cubemap(int size, TextureFormat? format, GraphicsFormat graphics, int mipCount, long rawSize = 0)
    {
        if (size < 1 || size > MaxSize) throw new ArgumentException($"Cubemap size {size} is outside 1 to {MaxSize}");
        if (mipCount != GenerateAllMips && mipCount < 1) throw new ArgumentException($"mipCount {mipCount} must be 1 or more, or Texture.GenerateAllMips");
        var full = TexelFormats.FullChain(size, size);
        (_size, _format, _graphics) = (size, format, graphics);
        _mipCount = mipCount == GenerateAllMips ? full : Math.Min(mipCount, full);
        var layout = format is { } known && TexelFormats.CanAllocate(known) && !TexelFormats.IsCrunched(known) ? format : null;
        _faceSize = layout is { } laid ? TexelFormats.ChainOffsets(laid, size, size, _mipCount)[^1] : 0;
        var total = layout is null ? rawSize : _faceSize * Faces;
        if (total > Array.MaxLength) throw new NotSupportedException($"Cubemap of {total} bytes is too large");
        _data = new byte[total];
    }

    public Cubemap(int width, TextureFormat textureFormat, bool mipChain) :
        this(width, textureFormat, GraphicsFormatUtility.GetGraphicsFormat(textureFormat, false), mipChain ? GenerateAllMips : 1) { }

    public Cubemap(int width, TextureFormat textureFormat, int mipCount) :
        this(width, textureFormat, GraphicsFormatUtility.GetGraphicsFormat(textureFormat, false), mipCount) { }

    public Cubemap(int width, GraphicsFormat format, TextureCreationFlags flags) :
        this(width, format, flags, (flags & TextureCreationFlags.MipChain) != 0 ? GenerateAllMips : 1) { }

    public Cubemap(int width, GraphicsFormat format, TextureCreationFlags flags, int mipCount) :
        this(width, GraphicsFormatUtility.GetTextureFormat(format) is var known && Enum.IsDefined(known) ? known : null, format, mipCount) { }

    public override int width { get => _size; set => base.width = value; }
    public override int height { get => _size; set => base.height = value; }
    public override TextureDimension dimension { get => TextureDimension.Cube; set => base.dimension = value; }
    public override int mipmapCount => _mipCount;
    internal override GraphicsFormat GraphicsFormatCore => _graphics;

    /// <summary>The format the cubemap was made with, or <see cref="TextureFormat.RGBA32"/> when it was made from a graphics format with no texture format.</summary>
    public TextureFormat format => _format ?? TextureFormat.RGBA32;

    /// <summary>The raw bytes of every face, which a player's pixels are read into.</summary>
    internal Span<byte> PixelBytes => _data;

    /// <summary>The size in bytes of every face with its whole mip chain, or zero for a format the engine has no layout for.</summary>
    internal int ByteLength => _data.Length;

    public Color GetPixel(CubemapFace face, int x, int y) => throw Unsupported(nameof(GetPixel));
    public void SetPixel(CubemapFace face, int x, int y, Color color) => throw Unsupported(nameof(SetPixel));
    public Color[] GetPixels(CubemapFace face) => throw Unsupported(nameof(GetPixels));
    public void SetPixels(Color[] colors, CubemapFace face) => throw Unsupported(nameof(SetPixels));
    public void Apply(bool updateMipmaps = true, bool makeNoLongerReadable = false) => throw Unsupported(nameof(Apply));

    private static NotSupportedException Unsupported(string member) => new($"Cubemap.{member} is not supported yet: the engine holds a cubemap's faces as bytes only");
}
