using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The base of Texture2D, Texture2DArray, Texture3D and RenderTexture. The engine keeps a texture's settings and
/// contents in managed memory. A Texture2D or Texture2DArray that a material, a property block or a sprite uses is
/// also stored with the host under a renderer identity, which draws from the stored copy; the native texture
/// accessors return no GPU object. The streaming and memory statistics read 0 because no texture is resident on a
/// GPU. The global quality settings are stored and read back.
/// </summary>
public abstract class Texture : Object
{
    public static readonly int GenerateAllMips = -1;

    private static int _globalMipmapLimit;
    private static AnisotropicFiltering _anisotropicFiltering = AnisotropicFiltering.Enable;
    private TextureWrapMode _wrapU, _wrapV, _wrapW;
    private FilterMode _filterMode = FilterMode.Bilinear;
    private int _anisoLevel = 1;
    private float _mipMapBias;

    protected Texture() { }

    private ulong _identity;
    private WorldGraphics.Link _stored;

    /// <summary>The renderer identity this texture has, or zero while it has none. Only a texture the host draws from has one.</summary>
    internal ulong StoredIdentity => _identity;

    /// <summary>The identity the renderer knows this texture by: allocated the first time something needs it (a sprite, a
    /// material slot or a property block), from <see cref="DisunityHost.FirstPlayerTextureIdentity"/> up, and kept for the
    /// texture's lifetime. Taking it stores the texture's bytes with the host.</summary>
    private protected ulong TakeRendererIdentity()
    {
        if (_identity != 0) return _identity;
        _identity = DisunityHost.AllocatePlayerTextureIdentity(this);
        StoreForRenderer();
        return _identity;
    }

    /// <summary>Hands the texture's raw bytes and description to the host, which draws from them: called when the texture
    /// takes its identity and when its contents are applied. A texture the host does not draw from has nothing to hand over.</summary>
    internal virtual void StoreForRenderer() { }

    /// <summary>Stores <paramref name="bytes"/> under the texture's identity with <paramref name="info"/>, whose identity and
    /// length this fills in, and sends the sampler, which the host keeps across later stores.</summary>
    private protected unsafe void StoreRecord(DisunityTextureInfo info, byte[] bytes)
    {
        DisunityHost.EnsureLoaded();
        info.identity = _identity;
        info.length = (uint)bytes.Length;
        fixed (byte* pointer = bytes)
            DisunityHost.ThrowIfError(NativeBindings.disunity_texture_store(DisunityHost.WorldHandle, _identity, &info, pointer, info.length));
        _stored = new WorldGraphics.Link(DisunityHost.WorldHandle, _identity);
        WriteSampler();
    }

    private unsafe void WriteSampler()
    {
        var sampler = Sampler;
        DisunityHost.ThrowIfError(NativeBindings.disunity_texture_sampler_write(DisunityHost.WorldHandle, _identity, &sampler));
    }

    [Obsolete("Use globalMipmapLimit instead.")]
    public static int masterTextureLimit { get => _globalMipmapLimit; set => _globalMipmapLimit = value; }
    public static int globalMipmapLimit { get => _globalMipmapLimit; set => _globalMipmapLimit = value; }
    public static AnisotropicFiltering anisotropicFiltering { get => _anisotropicFiltering; set => _anisotropicFiltering = value; }
    public static bool streamingTextureForceLoadAll { get; set; }
    public static bool streamingTextureDiscardUnusedMips { get; set; }
    public static bool allowThreadedTextureCreation { get; set; }

    public static ulong totalTextureMemory => 0;
    public static ulong desiredTextureMemory => 0;
    public static ulong targetTextureMemory => 0;
    public static ulong currentTextureMemory => 0;
    public static ulong nonStreamingTextureMemory => 0;
    public static ulong streamingMipmapUploadCount => 0;
    public static ulong streamingRendererCount => 0;
    public static ulong streamingTextureCount => 0;
    public static ulong nonStreamingTextureCount => 0;
    public static ulong streamingTexturePendingLoadCount => 0;
    public static ulong streamingTextureLoadingCount => 0;

    /// <summary>Limits are stored and no texture applies them.</summary>
    public static void SetGlobalAnisotropicFilteringLimits(int forcedMin, int globalMax)
    {
        ForcedMinimumAnisotropy = forcedMin;
        GlobalMaximumAnisotropy = globalMax;
    }

    internal static int ForcedMinimumAnisotropy { get; private set; } = -1;
    internal static int GlobalMaximumAnisotropy { get; private set; } = -1;

    /// <summary>A debug aid that tints materials by texture streaming state. The engine does not stream textures: every mip
    /// level is resident, as the <see cref="Texture2D"/> streaming members report, so there is no streaming state to show.</summary>
    public static void SetStreamingTextureMaterialDebugProperties() { }

    /// <inheritdoc cref="SetStreamingTextureMaterialDebugProperties()"/>
    public static void SetStreamingTextureMaterialDebugProperties(int materialTextureSlot) { }

    public virtual int width
    {
        get => 0;
        set => throw new NotImplementedException("Texture.width cannot be set on this kind of texture");
    }

    public virtual int height
    {
        get => 0;
        set => throw new NotImplementedException("Texture.height cannot be set on this kind of texture");
    }

    public virtual TextureDimension dimension
    {
        get => TextureDimension.None;
        set => throw new NotImplementedException("Texture.dimension cannot be set on this kind of texture");
    }

    public virtual int mipmapCount => 1;
    public GraphicsFormat graphicsFormat => GraphicsFormatCore;

    /// <summary>What <see cref="graphicsFormat"/> reads. RenderTexture declares its own settable
    /// graphicsFormat, so a Texture-typed reference must reach the format through this member.</summary>
    internal virtual GraphicsFormat GraphicsFormatCore => GraphicsFormat.None;

    public virtual bool isReadable => false;
    public bool isDataSRGB => GraphicsFormats.IsSrgb(GraphicsFormatCore);
    public Vector2 texelSize => new(1f / Math.Max(width, 1), 1f / Math.Max(height, 1));

    public TextureWrapMode wrapMode
    {
        get => _wrapU;
        set
        {
            _wrapU = _wrapV = _wrapW = value;
            SamplerChanged();
        }
    }

    public TextureWrapMode wrapModeU { get => _wrapU; set { _wrapU = value; SamplerChanged(); } }
    public TextureWrapMode wrapModeV { get => _wrapV; set { _wrapV = value; SamplerChanged(); } }
    public TextureWrapMode wrapModeW { get => _wrapW; set { _wrapW = value; SamplerChanged(); } }
    public FilterMode filterMode { get => _filterMode; set { _filterMode = value; SamplerChanged(); } }
    public int anisoLevel { get => _anisoLevel; set { _anisoLevel = value; SamplerChanged(); } }
    public float mipMapBias { get => _mipMapBias; set { _mipMapBias = value; SamplerChanged(); } }

    /// <summary>Called after a sampling setting changes, so a texture the host holds can send it again. A texture the host
    /// holds in the loaded world sends the sampler to it; one it does not hold yet sends it with its bytes.</summary>
    internal virtual void SamplerChanged()
    {
        if (_stored.IsCurrent) WriteSampler();
    }

    /// <summary>The sampling settings in the host's terms. The engine's choice for values the host cannot hold: a wrap
    /// mode, filter or anisotropy level outside its range clamps to the nearest one, and a bias that is not a number is 0.</summary>
    internal DisunitySampler Sampler => new()
    {
        wrap_u = Clamped((int)_wrapU, 3), wrap_v = Clamped((int)_wrapV, 3), wrap_w = Clamped((int)_wrapW, 3),
        filter = Clamped((int)_filterMode, 2), aniso_level = Clamped(_anisoLevel, 16),
        mip_bias = float.IsFinite(_mipMapBias) ? _mipMapBias : 0f,
    };

    private static uint Clamped(int value, int highest) => (uint)Math.Clamp(value, 0, highest);

    /// <summary>The number of times the contents changed, which a renderer compares to know when to upload.</summary>
    public uint updateCount { get; private set; }

    public void IncrementUpdateCount() => updateCount = unchecked(updateCount + 1);

    /// <summary>No GPU object exists, so this is always zero.</summary>
    public IntPtr GetNativeTexturePtr() => IntPtr.Zero;

    /// <summary>No GPU object exists, so this is always zero.</summary>
    public int GetNativeTextureID() => 0;

    public GraphicsTexture graphicsTexture =>
        throw new NotSupportedException("Texture.graphicsTexture is not supported yet: the engine has no GPU textures");

    /// <summary>Copies the sampling settings, which a cloned texture keeps.</summary>
    internal void CopySamplerFrom(Texture source)
    {
        _wrapU = source._wrapU;
        _wrapV = source._wrapV;
        _wrapW = source._wrapW;
        _filterMode = source._filterMode;
        _anisoLevel = source._anisoLevel;
        _mipMapBias = source._mipMapBias;
    }
}
