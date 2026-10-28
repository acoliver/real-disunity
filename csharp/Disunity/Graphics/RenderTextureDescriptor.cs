using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// Everything that describes a render texture. The color format is stored as a <see cref="GraphicsFormat"/>;
/// <see cref="colorFormat"/> converts through the tables in GraphicsFormats and throws
/// <see cref="NotSupportedException"/> for a format they do not list. A constructor given a mip count above one
/// turns <see cref="useMipMap"/> on. A default descriptor (not built by a constructor) has no size or format, as in Unity.
/// </summary>
public struct RenderTextureDescriptor
{
    private int _width, _height, _msaaSamples, _volumeDepth, _mipCount;
    private GraphicsFormat _graphicsFormat, _stencilFormat, _depthStencilFormat;
    private TextureDimension _dimension;
    private ShadowSamplingMode _shadowSamplingMode;
    private VRTextureUsage _vrUsage;
    private RenderTextureCreationFlags _flags;
    private RenderTextureMemoryless _memoryless;

    public RenderTextureDescriptor(int width, int height) : this(width, height, RenderTextureFormat.Default, 0) { }

    public RenderTextureDescriptor(int width, int height, RenderTextureFormat colorFormat) : this(width, height, colorFormat, 0) { }

    public RenderTextureDescriptor(int width, int height, RenderTextureFormat colorFormat, int depthBufferBits) :
        this(width, height, colorFormat, depthBufferBits, Texture.GenerateAllMips) { }

    public RenderTextureDescriptor(int width, int height, RenderTextureFormat colorFormat, int depthBufferBits, int mipCount) :
        this(width, height, colorFormat, depthBufferBits, mipCount, RenderTextureReadWrite.Default) { }

    public RenderTextureDescriptor(int width, int height, RenderTextureFormat colorFormat, int depthBufferBits, int mipCount,
        RenderTextureReadWrite readWrite) :
        this(width, height, GraphicsFormats.ColorFormat(colorFormat, GraphicsFormats.UsesSrgb(readWrite)),
            GraphicsFormats.DepthFormat(depthBufferBits), mipCount)
    {
        if (colorFormat == RenderTextureFormat.Shadowmap) _shadowSamplingMode = ShadowSamplingMode.CompareDepths;
    }

    public RenderTextureDescriptor(int width, int height, GraphicsFormat colorFormat, int depthBufferBits) :
        this(width, height, colorFormat, GraphicsFormats.DepthFormat(depthBufferBits), Texture.GenerateAllMips) { }

    public RenderTextureDescriptor(int width, int height, GraphicsFormat colorFormat, int depthBufferBits, int mipCount) :
        this(width, height, colorFormat, GraphicsFormats.DepthFormat(depthBufferBits), mipCount) { }

    public RenderTextureDescriptor(int width, int height, GraphicsFormat colorFormat, GraphicsFormat depthStencilFormat) :
        this(width, height, colorFormat, depthStencilFormat, Texture.GenerateAllMips) { }

    public RenderTextureDescriptor(int width, int height, GraphicsFormat colorFormat, GraphicsFormat depthStencilFormat, int mipCount)
    {
        _width = width;
        _height = height;
        _msaaSamples = 1;
        _volumeDepth = 1;
        _mipCount = mipCount;
        _graphicsFormat = colorFormat;
        _stencilFormat = GraphicsFormat.None;
        _depthStencilFormat = depthStencilFormat;
        _dimension = TextureDimension.Tex2D;
        _shadowSamplingMode = ShadowSamplingMode.None;
        _vrUsage = VRTextureUsage.None;
        _flags = RenderTextureCreationFlags.AutoGenerateMips | RenderTextureCreationFlags.AllowVerticalFlip;
        _memoryless = RenderTextureMemoryless.None;
        if (GraphicsFormats.IsSrgb(colorFormat)) _flags |= RenderTextureCreationFlags.SRGB;
        if (mipCount > 1) _flags |= RenderTextureCreationFlags.MipMap;
    }

    public int width { readonly get => _width; set => _width = value; }
    public int height { readonly get => _height; set => _height = value; }
    public int msaaSamples { readonly get => _msaaSamples; set => _msaaSamples = value; }
    public int volumeDepth { readonly get => _volumeDepth; set => _volumeDepth = value; }
    public int mipCount { readonly get => _mipCount; set => _mipCount = value; }

    public GraphicsFormat graphicsFormat
    {
        readonly get => _graphicsFormat;
        set
        {
            _graphicsFormat = value;
            SetFlag(RenderTextureCreationFlags.SRGB, GraphicsFormats.IsSrgb(value));
        }
    }

    public GraphicsFormat stencilFormat { readonly get => _stencilFormat; set => _stencilFormat = value; }
    public GraphicsFormat depthStencilFormat { readonly get => _depthStencilFormat; set => _depthStencilFormat = value; }

    /// <summary>The color format as a <see cref="RenderTextureFormat"/>; no color means Depth, or Shadowmap when the
    /// descriptor samples shadows. Setting Depth or Shadowmap drops the color and sets the shadow sampling to match.</summary>
    public RenderTextureFormat colorFormat
    {
        readonly get => GraphicsFormats.RenderFormat(_graphicsFormat, _shadowSamplingMode != ShadowSamplingMode.None);
        set
        {
            graphicsFormat = GraphicsFormats.ColorFormat(value, sRGB);
            if (value == RenderTextureFormat.Shadowmap) _shadowSamplingMode = ShadowSamplingMode.CompareDepths;
            else if (value == RenderTextureFormat.Depth) _shadowSamplingMode = ShadowSamplingMode.None;
        }
    }

    /// <summary>Whether the color is sRGB encoded. Setting it switches the color format to its sRGB or linear variant.</summary>
    public bool sRGB
    {
        readonly get => (_flags & RenderTextureCreationFlags.SRGB) != 0;
        set
        {
            _graphicsFormat = GraphicsFormats.WithSrgb(_graphicsFormat, value);
            SetFlag(RenderTextureCreationFlags.SRGB, value);
        }
    }

    /// <summary>The depth buffer's bits: 0, 16, 24 or 32. Setting another value throws <see cref="ArgumentException"/>.</summary>
    public int depthBufferBits
    {
        readonly get => GraphicsFormats.DepthBits(_depthStencilFormat);
        set => _depthStencilFormat = GraphicsFormats.DepthFormat(value);
    }

    public TextureDimension dimension { readonly get => _dimension; set => _dimension = value; }
    public ShadowSamplingMode shadowSamplingMode { readonly get => _shadowSamplingMode; set => _shadowSamplingMode = value; }
    public VRTextureUsage vrUsage { readonly get => _vrUsage; set => _vrUsage = value; }
    public readonly RenderTextureCreationFlags flags => _flags;
    public RenderTextureMemoryless memoryless { readonly get => _memoryless; set => _memoryless = value; }

    public bool useMipMap { readonly get => Has(RenderTextureCreationFlags.MipMap); set => SetFlag(RenderTextureCreationFlags.MipMap, value); }
    public bool autoGenerateMips { readonly get => Has(RenderTextureCreationFlags.AutoGenerateMips); set => SetFlag(RenderTextureCreationFlags.AutoGenerateMips, value); }
    public bool enableRandomWrite { readonly get => Has(RenderTextureCreationFlags.EnableRandomWrite); set => SetFlag(RenderTextureCreationFlags.EnableRandomWrite, value); }
    public bool bindMS { readonly get => Has(RenderTextureCreationFlags.BindMS); set => SetFlag(RenderTextureCreationFlags.BindMS, value); }
    public bool useDynamicScale { readonly get => Has(RenderTextureCreationFlags.DynamicallyScalable); set => SetFlag(RenderTextureCreationFlags.DynamicallyScalable, value); }
    public bool useDynamicScaleExplicit { readonly get => Has(RenderTextureCreationFlags.DynamicallyScalableExplicit); set => SetFlag(RenderTextureCreationFlags.DynamicallyScalableExplicit, value); }
    public bool enableShadingRate { readonly get => Has(RenderTextureCreationFlags.ShadingRate); set => SetFlag(RenderTextureCreationFlags.ShadingRate, value); }

    private readonly bool Has(RenderTextureCreationFlags flag) => (_flags & flag) != 0;

    private void SetFlag(RenderTextureCreationFlags flag, bool on) => _flags = on ? _flags | flag : _flags & ~flag;

    /// <summary>Whether two descriptors describe the same texture, which is how temporary textures are reused.</summary>
    internal static bool Same(in RenderTextureDescriptor a, in RenderTextureDescriptor b) =>
        a._width == b._width && a._height == b._height && a._msaaSamples == b._msaaSamples && a._volumeDepth == b._volumeDepth
        && a._mipCount == b._mipCount && a._graphicsFormat == b._graphicsFormat && a._stencilFormat == b._stencilFormat
        && a._depthStencilFormat == b._depthStencilFormat && a._dimension == b._dimension
        && a._shadowSamplingMode == b._shadowSamplingMode && a._vrUsage == b._vrUsage && a._flags == b._flags && a._memoryless == b._memoryless;
}
