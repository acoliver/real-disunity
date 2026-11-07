using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A texture of six square faces. The engine holds the size, the format and the mip count and none of the faces' pixels, and
/// nothing draws it: a pipeline refers to cubemaps for reflections and looks at their dimension. Every pixel member throws
/// <see cref="NotSupportedException"/> naming it. The mip count is clamped to the full chain of the size.
/// </summary>
public class Cubemap : Texture
{
    private const int MaxSize = 16384;

    private readonly int _size, _mipCount;
    private readonly GraphicsFormat _graphics;
    private readonly TextureFormat? _format;

    private Cubemap(int size, TextureFormat? format, GraphicsFormat graphics, int mipCount)
    {
        if (size < 1 || size > MaxSize) throw new ArgumentException($"Cubemap size {size} is outside 1 to {MaxSize}");
        if (mipCount != GenerateAllMips && mipCount < 1) throw new ArgumentException($"mipCount {mipCount} must be 1 or more, or Texture.GenerateAllMips");
        var full = TexelFormats.FullChain(size, size);
        (_size, _format, _graphics) = (size, format, graphics);
        _mipCount = mipCount == GenerateAllMips ? full : Math.Min(mipCount, full);
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

    public Color GetPixel(CubemapFace face, int x, int y) => throw Unsupported(nameof(GetPixel));
    public void SetPixel(CubemapFace face, int x, int y, Color color) => throw Unsupported(nameof(SetPixel));
    public Color[] GetPixels(CubemapFace face) => throw Unsupported(nameof(GetPixels));
    public void SetPixels(Color[] colors, CubemapFace face) => throw Unsupported(nameof(SetPixels));
    public void Apply(bool updateMipmaps = true, bool makeNoLongerReadable = false) => throw Unsupported(nameof(Apply));

    private static NotSupportedException Unsupported(string member) => new($"Cubemap.{member} is not supported yet: the engine holds no pixels of a cubemap's faces");
}
