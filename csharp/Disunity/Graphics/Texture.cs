using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The base of Texture2D and RenderTexture. The engine keeps a texture's settings and contents in managed
/// memory and nothing draws them yet; the native texture accessors return no GPU object. The streaming
/// and memory statistics read 0 because no texture is resident on a GPU. The global quality settings
/// are stored and read back.
/// </summary>
public abstract class Texture : Object
{
    public static readonly int GenerateAllMips = -1;

    private static int _globalMipmapLimit;
    private static AnisotropicFiltering _anisotropicFiltering = AnisotropicFiltering.Enable;
    private TextureWrapMode _wrapU, _wrapV, _wrapW;

    protected Texture() { }

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

    /// <summary>A debug aid that tints materials by streaming state; no streaming exists, so it does nothing.</summary>
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
        set => _wrapU = _wrapV = _wrapW = value;
    }

    public TextureWrapMode wrapModeU { get => _wrapU; set => _wrapU = value; }
    public TextureWrapMode wrapModeV { get => _wrapV; set => _wrapV = value; }
    public TextureWrapMode wrapModeW { get => _wrapW; set => _wrapW = value; }
    public FilterMode filterMode { get; set; } = FilterMode.Bilinear;
    public int anisoLevel { get; set; } = 1;
    public float mipMapBias { get; set; }

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
        filterMode = source.filterMode;
        anisoLevel = source.anisoLevel;
        mipMapBias = source.mipMapBias;
    }
}
