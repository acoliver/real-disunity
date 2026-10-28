using Disunity.Hosting;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Unity.Collections;
using Unity.Collections.LowLevel.Unsafe;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// A 2D texture whose pixels live in managed memory, in the raw layout of its format with the mip levels
/// stored one after another, level 0 first and each row from the bottom up, as Unity lays them out.
/// Formats with a codec (the 8-bit, 16-bit, half and float color formats and the packed 16-bit ones)
/// support pixel access. Compressed formats and the raw formats without a codec (the signed formats such as
/// R8_SNorm, and RGB9e5Float) keep bytes only and their pixel access throws <see cref="NotSupportedException"/>.
/// The bytes sit in a pinned array so the views from GetRawTextureData and GetPixelData share them. A view
/// stays valid while the texture is reachable and not destroyed; after Reinitialize it keeps pointing at the
/// bytes it was taken from, not at the new pixels, so take a new view. Nothing uploads the pixels to a GPU yet.
/// </summary>
public sealed partial class Texture2D : Texture
{
    private const int MaxSize = 16384;

    private static Texture2D? _white, _black, _red, _gray, _linearGray, _normal;

    private TextureFormat _format;
    private GraphicsFormat _graphics;
    private int _width, _height, _mipCount;
    private byte[] _data = [];
    private bool _viewed;
    private List<byte[]>? _retired;
    private long[] _offsets = [];
    private bool _readable = true;
    private string _limitGroup = "";
    private int _requestedMip = -1, _minimumMip;

    private Texture2D(int width, int height, TextureFormat format, GraphicsFormat graphics, int mipCount, string? limitGroup, bool ignoreLimit)
    {
        _limitGroup = limitGroup ?? "";
        ignoreMipmapLimit = ignoreLimit;
        Initialize(width, height, format, graphics, mipCount);
    }

    public Texture2D(int width, int height) : this(width, height, TextureFormat.RGBA32, true, false) { }

    public Texture2D(int width, int height, TextureFormat textureFormat, bool mipChain) : this(width, height, textureFormat, mipChain, false) { }

    public Texture2D(int width, int height, TextureFormat textureFormat, bool mipChain, bool linear) :
        this(width, height, textureFormat, GraphicsFormats.OfTexture(textureFormat, linear), mipChain ? GenerateAllMips : 1, null, false) { }

    public Texture2D(int width, int height, TextureFormat textureFormat, bool mipChain, bool linear, bool createUninitialized) :
        this(width, height, textureFormat, mipChain, linear) { }

    public Texture2D(int width, int height, TextureFormat textureFormat, int mipCount, bool linear) :
        this(width, height, textureFormat, GraphicsFormats.OfTexture(textureFormat, linear), mipCount, null, false) { }

    public Texture2D(int width, int height, TextureFormat textureFormat, int mipCount, bool linear, bool createUninitialized) :
        this(width, height, textureFormat, mipCount, linear) { }

    public Texture2D(int width, int height, TextureFormat textureFormat, int mipCount, bool linear, bool createUninitialized,
        MipmapLimitDescriptor mipmapLimitDescriptor) :
        this(width, height, textureFormat, GraphicsFormats.OfTexture(textureFormat, linear), mipCount,
            mipmapLimitDescriptor.groupName, !mipmapLimitDescriptor.useMipmapLimit) { }

    public Texture2D(int width, int height, TextureFormat textureFormat, int mipCount, bool linear, bool createUninitialized,
        bool ignoreMipmapLimit, string mipmapLimitGroupName) :
        this(width, height, textureFormat, GraphicsFormats.OfTexture(textureFormat, linear), mipCount, mipmapLimitGroupName, ignoreMipmapLimit) { }

    public Texture2D(int width, int height, GraphicsFormat format, TextureCreationFlags flags) :
        this(width, height, format, MipsOf(flags), flags) { }

    public Texture2D(int width, int height, GraphicsFormat format, int mipCount, TextureCreationFlags flags) :
        this(width, height, GraphicsFormats.ToTextureFormat(format), format, mipCount, null, flags.HasFlag(TextureCreationFlags.IgnoreMipmapLimit)) { }

    public Texture2D(int width, int height, GraphicsFormat format, int mipCount, string mipmapLimitGroupName, TextureCreationFlags flags) :
        this(width, height, GraphicsFormats.ToTextureFormat(format), format, mipCount, mipmapLimitGroupName, flags.HasFlag(TextureCreationFlags.IgnoreMipmapLimit)) { }

    public Texture2D(int width, int height, GraphicsFormat format, int mipCount, TextureCreationFlags flags,
        MipmapLimitDescriptor mipmapLimitDescriptor) :
        this(width, height, GraphicsFormats.ToTextureFormat(format), format, mipCount, mipmapLimitDescriptor.groupName, !mipmapLimitDescriptor.useMipmapLimit) { }

    public Texture2D(int width, int height, DefaultFormat format, TextureCreationFlags flags) :
        this(width, height, GraphicsFormats.DefaultColor(format), MipsOf(flags), flags) { }

    public Texture2D(int width, int height, DefaultFormat format, int mipCount, TextureCreationFlags flags) :
        this(width, height, GraphicsFormats.DefaultColor(format), mipCount, flags) { }

    public Texture2D(int width, int height, DefaultFormat format, int mipCount, string mipmapLimitGroupName, TextureCreationFlags flags) :
        this(width, height, GraphicsFormats.DefaultColor(format), mipCount, mipmapLimitGroupName, flags) { }

    public Texture2D(int width, int height, DefaultFormat format, int mipCount, TextureCreationFlags flags,
        MipmapLimitDescriptor mipmapLimitDescriptor) :
        this(width, height, GraphicsFormats.DefaultColor(format), mipCount, flags, mipmapLimitDescriptor) { }

    private static int MipsOf(TextureCreationFlags flags) => flags.HasFlag(TextureCreationFlags.MipChain) ? GenerateAllMips : 1;

    private void Initialize(int width, int height, TextureFormat format, GraphicsFormat graphics, int mipCount)
    {
        if (width <= 0 || height <= 0 || width > MaxSize || height > MaxSize)
            throw new ArgumentException($"Texture2D size {width}x{height} is outside 1 to {MaxSize}");
        if (TexelFormats.IsCrunched(format) || !TexelFormats.CanAllocate(format))
            throw new NotSupportedException($"Texture2D cannot be created in TextureFormat.{format} yet");
        var full = TexelFormats.FullChain(width, height);
        if (mipCount != GenerateAllMips && mipCount < 1) throw new ArgumentException($"mipCount {mipCount} must be 1 or more, or Texture.GenerateAllMips");
        var mips = mipCount == GenerateAllMips ? full : Math.Min(mipCount, full);
        var offsets = new long[mips + 1];
        for (var level = 0; level < mips; level++)
            offsets[level + 1] = offsets[level] + TexelFormats.LevelSize(format, Math.Max(1, width >> level), Math.Max(1, height >> level));
        if (offsets[mips] > Array.MaxLength) throw new NotSupportedException($"Texture2D of {offsets[mips]} bytes is too large");
        if (_viewed && _data.Length > 0) (_retired ??= []).Add(_data);
        _viewed = false;
        _data = GC.AllocateArray<byte>((int)offsets[mips], pinned: true);
        (_width, _height, _format, _graphics, _mipCount, _offsets) = (width, height, format, graphics, mips, offsets);
    }

    public override int width => _width;
    public override int height => _height;
    public override TextureDimension dimension => TextureDimension.Tex2D;
    public override int mipmapCount => _mipCount;
    internal override GraphicsFormat GraphicsFormatCore => _graphics;
    public override bool isReadable => _readable;
    public TextureFormat format => _format;

    /// <summary>The mipmap limit group the texture was created with, or an empty string; no limit is applied.</summary>
    public string mipmapLimitGroup => _limitGroup;
    public bool ignoreMipmapLimit { get; set; }

    /// <summary>Mip limits are not applied, so every level is active.</summary>
    public int activeMipmapLimit => 0;

    public bool vtOnly => false;
    public bool streamingMipmaps => false;
    public int streamingMipmapsPriority => 0;

    /// <summary>Streaming is not simulated: every level is resident, so the loaded, loading, desired and
    /// calculated levels are 0. The requested and minimum levels are stored.</summary>
    public int requestedMipmapLevel { get => _requestedMip; set => _requestedMip = value; }
    public int minimumMipmapLevel { get => _minimumMip; set => _minimumMip = value; }
    public int calculatedMipmapLevel => 0;
    public int desiredMipmapLevel => 0;
    public int loadingMipmapLevel => 0;
    public int loadedMipmapLevel => 0;
    public void ClearRequestedMipmapLevel() => _requestedMip = -1;
    public bool IsRequestedMipmapLevelLoaded() => true;
    public void ClearMinimumMipmapLevel() => _minimumMip = 0;

    public static Texture2D whiteTexture => Builtin(ref _white, "UnityWhite", new Color32(255, 255, 255, 255), linear: false);
    public static Texture2D blackTexture => Builtin(ref _black, "UnityBlack", new Color32(0, 0, 0, 255), linear: false);
    public static Texture2D redTexture => Builtin(ref _red, "UnityRed", new Color32(255, 0, 0, 255), linear: false);
    public static Texture2D grayTexture => Builtin(ref _gray, "UnityGray", new Color32(128, 128, 128, 255), linear: false);
    public static Texture2D linearGrayTexture => Builtin(ref _linearGray, "UnityLinearGray", new Color32(128, 128, 128, 255), linear: true);
    public static Texture2D normalTexture => Builtin(ref _normal, "UnityNormalMap", new Color32(128, 128, 255, 128), linear: true);

    /// <summary>A 4x4 texture of one color, made again if the previous one was destroyed. It is not readable,
    /// so a game cannot change a texture every other user of the built-in shares.</summary>
    private static Texture2D Builtin(ref Texture2D? slot, string name, Color32 color, bool linear)
    {
        DisunityHost.RequireMainThread("Texture2D built-in textures");
        if (slot is { } live && live) return live;
        var texture = new Texture2D(4, 4, TextureFormat.RGBA32, false, linear) { name = name, hideFlags = HideFlags.HideAndDontSave };
        texture.SetPixels32(Enumerable.Repeat(color, 16).ToArray());
        texture.Apply(false, true);
        return slot = texture;
    }

    /// <summary>The bytes of mip level <paramref name="level"/>, which must exist.</summary>
    private Span<byte> LevelBytes(int level) => _data.AsSpan((int)_offsets[level], (int)(_offsets[level + 1] - _offsets[level]));

    private int LevelWidth(int level) => Math.Max(1, _width >> level);
    private int LevelHeight(int level) => Math.Max(1, _height >> level);

    private int CheckLevel(int level, string member)
    {
        if (level < 0 || level >= _mipCount) throw new ArgumentException($"Texture2D.{member}: mip level {level} is outside 0 to {_mipCount - 1}");
        return level;
    }

    private void RequireReadable(string member)
    {
        if (!_readable) throw new InvalidOperationException($"Texture2D.{member}: texture '{name}' is not readable");
    }

    private void RequirePixels(string member)
    {
        RequireReadable(member);
        if (!TexelFormats.HasCodec(_format)) throw new NotSupportedException($"Texture2D.{member} is not supported yet for TextureFormat.{_format}");
    }

    internal override Object CloneAsset()
    {
        var copy = new Texture2D(_width, _height, _format, _graphics, _mipCount, _limitGroup, ignoreMipmapLimit) { _readable = _readable };
        _data.CopyTo(copy._data, 0);
        copy.CopySamplerFrom(this);
        return Named(copy);
    }

    public bool Reinitialize(int width, int height) => Reinitialize(width, height, _format, _mipCount > 1);

    public bool Reinitialize(int width, int height, TextureFormat format, bool hasMipMap)
    {
        Initialize(width, height, format, GraphicsFormats.OfTexture(format, !GraphicsFormats.IsSrgb(_graphics)), hasMipMap ? GenerateAllMips : 1);
        return true;
    }

    public bool Reinitialize(int width, int height, GraphicsFormat format, bool hasMipMap)
    {
        Initialize(width, height, GraphicsFormats.ToTextureFormat(format), format, hasMipMap ? GenerateAllMips : 1);
        return true;
    }

    public bool Resize(int width, int height) => Reinitialize(width, height);

    public bool Resize(int width, int height, TextureFormat format, bool hasMipMap) => Reinitialize(width, height, format, hasMipMap);

    public bool Resize(int width, int height, GraphicsFormat format, bool hasMipMap) => Reinitialize(width, height, format, hasMipMap);

    public byte[] GetRawTextureData()
    {
        RequireReadable(nameof(GetRawTextureData));
        return (byte[])_data.Clone();
    }

    public NativeArray<T> GetRawTextureData<T>() where T : struct
    {
        RequireReadable(nameof(GetRawTextureData));
        return View<T>(0, _data.Length);
    }

    public NativeArray<T> GetPixelData<T>(int mipLevel) where T : struct
    {
        RequireReadable(nameof(GetPixelData));
        var level = CheckLevel(mipLevel, nameof(GetPixelData));
        return View<T>((int)_offsets[level], (int)(_offsets[level + 1] - _offsets[level]));
    }

    internal override void OnAssetDestroyed() => _retired = null;

    private unsafe NativeArray<T> View<T>(int offset, int bytes) where T : struct
    {
        _viewed = true;
        var start = (byte*)Unsafe.AsPointer(ref MemoryMarshal.GetArrayDataReference(_data)) + offset;
        return NativeArrayUnsafeUtility.ConvertExistingDataToNativeArray<T>(start, bytes / UnsafeUtility.SizeOf<T>(), Allocator.None);
    }

    public void LoadRawTextureData(byte[] data)
    {
        ArgumentNullException.ThrowIfNull(data);
        LoadRaw(data);
    }

    public unsafe void LoadRawTextureData(IntPtr data, int size)
    {
        if (data == IntPtr.Zero) throw new ArgumentNullException(nameof(data));
        LoadRaw(new ReadOnlySpan<byte>((void*)data, size));
    }

    public unsafe void LoadRawTextureData<T>(NativeArray<T> data) where T : struct =>
        LoadRaw(new ReadOnlySpan<byte>(data.GetUnsafeReadOnlyPtr(), data.Length * UnsafeUtility.SizeOf<T>()));

    private void LoadRaw(ReadOnlySpan<byte> data)
    {
        RequireReadable(nameof(LoadRawTextureData));
        if (data.Length != _data.Length)
            throw new ArgumentException($"Texture2D.LoadRawTextureData: {data.Length} bytes given, the texture holds {_data.Length}");
        data.CopyTo(_data);
    }

    public void SetPixelData<T>(T[] data, int mipLevel, int sourceDataStartIndex = 0) where T : struct
    {
        ArgumentNullException.ThrowIfNull(data);
        SetData(MemoryMarshal.AsBytes(data.AsSpan()), UnsafeUtility.SizeOf<T>(), mipLevel, sourceDataStartIndex);
    }

    public unsafe void SetPixelData<T>(NativeArray<T> data, int mipLevel, int sourceDataStartIndex = 0) where T : struct =>
        SetData(new ReadOnlySpan<byte>(data.GetUnsafeReadOnlyPtr(), data.Length * UnsafeUtility.SizeOf<T>()), UnsafeUtility.SizeOf<T>(), mipLevel, sourceDataStartIndex);

    private void SetData(ReadOnlySpan<byte> source, int elementSize, int mipLevel, int startIndex)
    {
        RequireReadable(nameof(SetPixelData));
        var level = CheckLevel(mipLevel, nameof(SetPixelData));
        var target = LevelBytes(level);
        var from = checked((long)startIndex * elementSize);
        if (startIndex < 0 || source.Length - from < target.Length)
            throw new ArgumentException($"Texture2D.SetPixelData: mip {level} needs {target.Length} bytes from element {startIndex}, {Math.Max(0, source.Length - from)} given");
        source.Slice((int)from, target.Length).CopyTo(target);
    }
}
