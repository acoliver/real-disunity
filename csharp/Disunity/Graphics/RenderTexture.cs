using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A texture a camera or command buffer draws into. The engine keeps its description and whether it is
/// created, and holds no pixels in managed memory. The world has a render texture for it once something needs one (<see cref="Create"/>,
/// a camera target, a blit or a material property) and its description follows <see cref="Create"/>; the world's
/// render texture lives until the texture is destroyed, so <see cref="Release"/> and <see cref="Create"/> keep its id. A texture that is created
/// rejects changes to its size, format and other allocation settings with <see cref="InvalidOperationException"/>
/// until Release; the engine reports a late change instead of ignoring it. The same holds for ReleaseTemporary of a texture GetTemporary did not hand out or already got
/// back (<see cref="ArgumentException"/>), and for reading <see cref="format"/> when the graphics format has no
/// RenderTextureFormat (<see cref="NotSupportedException"/>). DiscardContents, MarkRestoreExpected,
/// ApplyDynamicScale and ResolveAntiAliasedSurface are hints or work on drawn contents, so they have no effect.
/// </summary>
public partial class RenderTexture : Texture
{
    private static RenderTexture? _active;

    private RenderTextureDescriptor _desc;
    private WorldGraphics.Link _world;
    private bool _created;
    private bool? _isPowerOfTwo;

    public RenderTexture(RenderTextureDescriptor desc)
    {
        Validate(desc);
        _desc = desc;
        wrapMode = TextureWrapMode.Clamp;
    }

    public RenderTexture(RenderTexture textureToCopy) : this(Source(textureToCopy)._desc) => CopySamplerFrom(textureToCopy);

    public RenderTexture(int width, int height, int depth, DefaultFormat format) : this(Describe(width, height, depth, format)) { }

    public RenderTexture(int width, int height, int depth, GraphicsFormat format) :
        this(new RenderTextureDescriptor(width, height, format, depth)) { }

    public RenderTexture(int width, int height, int depth, GraphicsFormat format, int mipCount) :
        this(new RenderTextureDescriptor(width, height, format, depth, mipCount)) { }

    public RenderTexture(int width, int height, GraphicsFormat colorFormat, GraphicsFormat depthStencilFormat, int mipCount) :
        this(new RenderTextureDescriptor(width, height, colorFormat, depthStencilFormat, mipCount)) { }

    public RenderTexture(int width, int height, GraphicsFormat colorFormat, GraphicsFormat depthStencilFormat) :
        this(new RenderTextureDescriptor(width, height, colorFormat, depthStencilFormat)) { }

    public RenderTexture(int width, int height, int depth, RenderTextureFormat format, RenderTextureReadWrite readWrite) :
        this(new RenderTextureDescriptor(width, height, format, depth, Texture.GenerateAllMips, readWrite)) { }

    public RenderTexture(int width, int height, int depth, RenderTextureFormat format) :
        this(new RenderTextureDescriptor(width, height, format, depth)) { }

    public RenderTexture(int width, int height, int depth) : this(width, height, depth, RenderTextureFormat.Default) { }

    public RenderTexture(int width, int height, int depth, RenderTextureFormat format, int mipCount) :
        this(new RenderTextureDescriptor(width, height, format, depth, mipCount)) { }

    private static RenderTexture Source(RenderTexture texture)
    {
        ArgumentNullException.ThrowIfNull(texture);
        return texture;
    }

    private static RenderTextureDescriptor Describe(int width, int height, int depth, DefaultFormat format)
    {
        var descriptor = new RenderTextureDescriptor(width, height, GraphicsFormats.DefaultColor(format), depth);
        if (format == DefaultFormat.Shadow) descriptor.shadowSamplingMode = ShadowSamplingMode.CompareDepths;
        return descriptor;
    }

    private static void Validate(in RenderTextureDescriptor desc)
    {
        if (desc.width <= 0) throw new ArgumentException("RenderTexture width must be greater than zero", "desc.width");
        if (desc.height <= 0) throw new ArgumentException("RenderTexture height must be greater than zero", "desc.height");
        if (desc.volumeDepth < 1) throw new ArgumentException("RenderTexture volumeDepth must be at least one", "desc.volumeDepth");
        if (desc.dimension is not (TextureDimension.Tex2D or TextureDimension.Tex3D or TextureDimension.Cube or TextureDimension.Tex2DArray or TextureDimension.CubeArray))
            throw new ArgumentException($"RenderTexture dimension {desc.dimension} is not a dimension a render texture can have", "desc.dimension");
        if (desc.msaaSamples is not (1 or 2 or 4 or 8)) throw new ArgumentException("RenderTexture antiAliasing must be 1, 2, 4 or 8", "desc.msaaSamples");
    }

    /// <summary>Changing a created texture's allocation is refused until it is released.</summary>
    private void Change(string member)
    {
        if (_created) throw new InvalidOperationException($"RenderTexture.{member} cannot change while the texture is created; call Release first");
    }

    public override int width { get => _desc.width; set { Change(nameof(width)); _desc.width = value; } }
    public override int height { get => _desc.height; set { Change(nameof(height)); _desc.height = value; } }
    public override TextureDimension dimension { get => _desc.dimension; set { Change(nameof(dimension)); _desc.dimension = value; } }
    public int volumeDepth { get => _desc.volumeDepth; set { Change(nameof(volumeDepth)); _desc.volumeDepth = value; } }
    public int antiAliasing { get => _desc.msaaSamples; set => SetAntiAliasing(value); }
    internal override GraphicsFormat GraphicsFormatCore => _desc.graphicsFormat;
    public new GraphicsFormat graphicsFormat { get => _desc.graphicsFormat; set { Change(nameof(graphicsFormat)); _desc.graphicsFormat = value; } }
    public GraphicsFormat stencilFormat { get => _desc.stencilFormat; set { Change(nameof(stencilFormat)); _desc.stencilFormat = value; } }
    public GraphicsFormat depthStencilFormat { get => _desc.depthStencilFormat; set { Change(nameof(depthStencilFormat)); _desc.depthStencilFormat = value; } }
    public RenderTextureFormat format { get => _desc.colorFormat; set { Change(nameof(format)); _desc.colorFormat = value; } }
    public bool sRGB => _desc.sRGB;
    public int depth { get => _desc.depthBufferBits; set { Change(nameof(depth)); _desc.depthBufferBits = value; } }
    public bool useMipMap { get => _desc.useMipMap; set { Change(nameof(useMipMap)); _desc.useMipMap = value; } }
    public bool autoGenerateMips { get => _desc.autoGenerateMips; set => _desc.autoGenerateMips = value; }
    public bool enableRandomWrite { get => _desc.enableRandomWrite; set { Change(nameof(enableRandomWrite)); _desc.enableRandomWrite = value; } }
    public bool bindTextureMS { get => _desc.bindMS; set { Change(nameof(bindTextureMS)); _desc.bindMS = value; } }
    public bool useDynamicScale { get => _desc.useDynamicScale; set { Change(nameof(useDynamicScale)); _desc.useDynamicScale = value; } }
    public bool useDynamicScaleExplicit { get => _desc.useDynamicScaleExplicit; set => _desc.useDynamicScaleExplicit = value; }
    public bool enableShadingRate { get => _desc.enableShadingRate; set => _desc.enableShadingRate = value; }
    public VRTextureUsage vrUsage { get => _desc.vrUsage; set { Change(nameof(vrUsage)); _desc.vrUsage = value; } }
    public RenderTextureMemoryless memorylessMode { get => _desc.memoryless; set { Change(nameof(memorylessMode)); _desc.memoryless = value; } }
    public bool isPowerOfTwo { get => _isPowerOfTwo ?? (IsPowerOfTwo(width) && IsPowerOfTwo(height)); set => _isPowerOfTwo = value; }
    public bool isCubemap { get => dimension == TextureDimension.Cube; set => dimension = value ? TextureDimension.Cube : TextureDimension.Tex2D; }
    public bool isVolume { get => dimension == TextureDimension.Tex3D; set => dimension = value ? TextureDimension.Tex3D : TextureDimension.Tex2D; }

    /// <summary>The levels of the chain when mips are on and a count was asked for, otherwise one.</summary>
    public override int mipmapCount
    {
        get
        {
            if (!_desc.useMipMap) return 1;
            var full = TexelFormats.FullChain(Math.Max(width, 1), Math.Max(height, 1));
            return _desc.mipCount < 1 ? full : Math.Min(_desc.mipCount, full);
        }
    }

    private static bool IsPowerOfTwo(int value) => value > 0 && (value & (value - 1)) == 0;

    private void SetAntiAliasing(int value)
    {
        Change(nameof(antiAliasing));
        if (value is not (1 or 2 or 4 or 8)) throw new ArgumentOutOfRangeException(nameof(antiAliasing), "antiAliasing must be 1, 2, 4 or 8");
        _desc.msaaSamples = value;
    }

    public RenderTextureDescriptor descriptor
    {
        get => _desc;
        set
        {
            Change(nameof(descriptor));
            Validate(value);
            _desc = value;
        }
    }

    /// <summary>The color surface's token; see <see cref="RenderBuffer"/>.</summary>
    public RenderBuffer colorBuffer => Buffer(0);

    public RenderBuffer depthBuffer => Buffer(1);

    private RenderBuffer Buffer(long surface) => new() { m_RenderTextureInstanceID = GetInstanceID(), m_BufferPtr = new IntPtr(2L * GetInstanceID() + surface) };

    /// <summary>No GPU object exists, so this is always zero.</summary>
    public IntPtr GetNativeDepthBufferPtr() => IntPtr.Zero;

    /// <summary>The texture drawing goes to when no other target is set. Setting a texture creates it.</summary>
    public static RenderTexture? active
    {
        get
        {
            DisunityHost.RequireMainThread("RenderTexture.active");
            return _active is { } current && current ? current : null;
        }
        set
        {
            DisunityHost.RequireMainThread("RenderTexture.active");
            if (value is not null && value && !value._created) value.Create();
            _active = value;
        }
    }

    /// <summary>Kept as state; no code reads it because render textures are always usable.</summary>
    public static bool enabled { get; set; } = true;

    public bool Create()
    {
        if (_desc.width <= 0 || _desc.height <= 0) return false;
        _created = true;
        if (!DisunityHost.IsLoaded) return true;
        if (_world.IsCurrent) WorldGraphics.WriteRenderTexture(_world, Describe());
        else _ = WorldLink;
        return true;
    }

    /// <summary>The texture's render texture in the loaded world, made on first use from the description at that time.</summary>
    internal WorldGraphics.Link WorldLink
    {
        get
        {
            if (_world.IsCurrent) return _world;
            _world = WorldGraphics.CreateRenderTexture(Describe());
            return _world;
        }
    }

    /// <summary>The id of <see cref="WorldLink"/>.</summary>
    internal ulong WorldId => WorldLink.Id;

    /// <summary>The world's description: the depth bits are the texture's (0, 16, 24 or 32) and the mip count is that of the chain, at most 15.</summary>
    /// <summary>Whether the dimension has more than one slice, layer or face set: a 3D texture or an array. A 2D texture or
    /// a cube ignores <see cref="volumeDepth"/>.</summary>
    private bool Shaped => dimension is TextureDimension.Tex3D or TextureDimension.Tex2DArray or TextureDimension.CubeArray;

    internal DisunityRenderTexture Describe() => new()
    {
        width = (uint)width, height = (uint)height, format = (uint)(int)_desc.graphicsFormat,
        depth_bits = (uint)Math.Max(depth, 0),
        mip_count = (uint)Math.Clamp(mipmapCount, 1, 15), srgb = sRGB ? 1u : 0u,
        dimension = (uint)(int)this.dimension, volume_depth = Shaped ? (uint)Math.Max(volumeDepth, 1) : 1u,
        wrap_u = Sampler.wrap_u, wrap_v = Sampler.wrap_v, wrap_w = Sampler.wrap_w, filter = Sampler.filter,
        aniso_level = Sampler.aniso_level, mip_bias = Sampler.mip_bias,
    };

    /// <summary>Sends the description again, with the new sampler, once the world holds a render texture for this one.</summary>
    internal override void SamplerChanged()
    {
        if (_world.IsCurrent) WorldGraphics.WriteRenderTexture(_world, Describe());
    }

    public void Release()
    {
        _created = false;
        if (ReferenceEquals(_active, this)) _active = null;
    }

    public bool IsCreated() => _created;

    internal override void OnAssetDestroyed()
    {
        Release();
        if (!_world.IsCurrent) return;
        WorldGraphics.DestroyRenderTexture(_world);
        _world = default;
    }

    public void DiscardContents() { }

    public void DiscardContents(bool discardColor, bool discardDepth) { }

    public void MarkRestoreExpected() { }

    public void ApplyDynamicScale() { }

    public void ResolveAntiAliasedSurface() { }

    public void ResolveAntiAliasedSurface(RenderTexture target) => ArgumentNullException.ThrowIfNull(target);

    /// <summary>Counts an update; there are no pixels to filter. Logs an error, as Unity does, unless the texture has mips it does not generate itself.</summary>
    public void GenerateMips()
    {
        if (!_desc.useMipMap || _desc.autoGenerateMips)
        {
            Debug.LogError("RenderTexture.GenerateMips needs useMipMap on and autoGenerateMips off", this);
            return;
        }
        IncrementUpdateCount();
    }

    public void SetGlobalShaderProperty(string propertyName) => Shader.SetGlobalTexture(propertyName, this);

    public void ConvertToEquirect(RenderTexture equirect, Camera.MonoOrStereoscopicEye eye = Camera.MonoOrStereoscopicEye.Mono) =>
        throw new NotSupportedException("RenderTexture.ConvertToEquirect is not supported yet");

    public static bool SupportsStencil(RenderTexture rt)
    {
        ArgumentNullException.ThrowIfNull(rt);
        return GraphicsFormats.HasStencil(rt.depthStencilFormat) || rt.stencilFormat != GraphicsFormat.None;
    }

    /// <summary>Unity's texel offset for half-pixel platforms; none of the engine's backends have one.</summary>
    public Vector2 GetTexelOffset() => Vector2.zero;

    internal override Object CloneAsset() => Named(new RenderTexture(this));
}
