using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>RenderTextureDescriptor, RenderTexture, temporary textures, format mapping and RenderTargetIdentifier.</summary>
internal static class RenderTextureTests
{
    public static void Run()
    {
        Descriptors();
        FormatMapping();
        DescriptorRoundTrip();
        Lifecycle();
        LifecycleHints();
        ReadThroughTexture();
        ActiveTexture();
        Temporaries();
        TemporaryPoolLimit();
        Settings();
        Identifiers();
        Console.WriteLine("PASS: RenderTexture descriptors, format mapping, create/release, active, temporary pool and RenderTargetIdentifier");
    }

    private static void Descriptors()
    {
        var descriptor = new RenderTextureDescriptor(256, 128, RenderTextureFormat.ARGB32, 24);
        Ensure(descriptor.width == 256 && descriptor.height == 128 && descriptor.depthBufferBits == 24, "size and depth bits");
        Ensure(descriptor.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && descriptor.sRGB && descriptor.colorFormat == RenderTextureFormat.ARGB32, "ARGB32 follows the project's linear color space");
        Ensure(descriptor.depthStencilFormat == GraphicsFormat.D24_UNorm_S8_UInt && descriptor.stencilFormat == GraphicsFormat.None, "24 depth bits");
        Ensure(descriptor.msaaSamples == 1 && descriptor.volumeDepth == 1 && descriptor.dimension == TextureDimension.Tex2D && descriptor.mipCount == -1, "defaults");
        Ensure(descriptor.autoGenerateMips && !descriptor.useMipMap && !descriptor.enableRandomWrite && !descriptor.bindMS && !descriptor.useDynamicScale, "default flags");
        Ensure(descriptor.shadowSamplingMode == ShadowSamplingMode.None && descriptor.vrUsage == VRTextureUsage.None && descriptor.memoryless == RenderTextureMemoryless.None, "other defaults");

        var linear = new RenderTextureDescriptor(8, 8, RenderTextureFormat.ARGB32, 0, 1, RenderTextureReadWrite.Linear);
        Ensure(linear.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm && !linear.sRGB, "ReadWrite.Linear is UNorm");
        Ensure(new RenderTextureDescriptor(8, 8, RenderTextureFormat.ARGB32, 0, 1, RenderTextureReadWrite.sRGB).graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, "ReadWrite.sRGB");
        Ensure(new RenderTextureDescriptor(8, 8, RenderTextureFormat.ARGB32, 0, 5).useMipMap, "a mip count above one turns mips on");
        Ensure(new RenderTextureDescriptor(8, 8).graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && new RenderTextureDescriptor(8, 8, RenderTextureFormat.Default).depthBufferBits == 0, "the short constructors");
        Ensure(new RenderTextureDescriptor(8, 8, GraphicsFormat.R16_SFloat, 16) is { depthStencilFormat: GraphicsFormat.D16_UNorm, depthBufferBits: 16, graphicsFormat: GraphicsFormat.R16_SFloat }, "a GraphicsFormat constructor with depth bits");
        Ensure(new RenderTextureDescriptor(8, 8, GraphicsFormat.R16_SFloat, 32, 3) is { depthStencilFormat: GraphicsFormat.D32_SFloat, mipCount: 3, useMipMap: true }, "a GraphicsFormat constructor with a mip count");
        Ensure(new RenderTextureDescriptor(8, 8, GraphicsFormat.R8_UNorm, GraphicsFormat.D32_SFloat_S8_UInt) is { depthBufferBits: 32 }, "a depth stencil format constructor");
        Ensure(new RenderTextureDescriptor(8, 8, GraphicsFormat.R8_UNorm, GraphicsFormat.D16_UNorm, 2).mipCount == 2, "a depth stencil format with mips");

        descriptor.msaaSamples = 4;
        descriptor.volumeDepth = 3;
        descriptor.useMipMap = true;
        descriptor.autoGenerateMips = false;
        descriptor.enableRandomWrite = true;
        descriptor.bindMS = true;
        descriptor.useDynamicScale = true;
        descriptor.useDynamicScaleExplicit = true;
        descriptor.enableShadingRate = true;
        descriptor.vrUsage = VRTextureUsage.TwoEyes;
        descriptor.memoryless = RenderTextureMemoryless.Color;
        descriptor.dimension = TextureDimension.Tex2DArray;
        descriptor.shadowSamplingMode = ShadowSamplingMode.RawDepth;
        descriptor.stencilFormat = GraphicsFormat.S8_UInt;
        descriptor.mipCount = 6;
        Ensure(descriptor is { msaaSamples: 4, volumeDepth: 3, useMipMap: true, autoGenerateMips: false, enableRandomWrite: true, bindMS: true, useDynamicScale: true, useDynamicScaleExplicit: true, enableShadingRate: true, mipCount: 6 }, "flag properties");
        Ensure(descriptor is { vrUsage: VRTextureUsage.TwoEyes, memoryless: RenderTextureMemoryless.Color, dimension: TextureDimension.Tex2DArray, shadowSamplingMode: ShadowSamplingMode.RawDepth, stencilFormat: GraphicsFormat.S8_UInt }, "value properties");
        Ensure((descriptor.flags & RenderTextureCreationFlags.MipMap) != 0 && (descriptor.flags & RenderTextureCreationFlags.AutoGenerateMips) == 0
            && (descriptor.flags & RenderTextureCreationFlags.EnableRandomWrite) != 0 && (descriptor.flags & RenderTextureCreationFlags.BindMS) != 0
            && (descriptor.flags & RenderTextureCreationFlags.DynamicallyScalable) != 0 && (descriptor.flags & RenderTextureCreationFlags.ShadingRate) != 0, "the flags value");
        descriptor.depthBufferBits = 16;
        Ensure(descriptor.depthStencilFormat == GraphicsFormat.D16_UNorm, "depthBufferBits sets the depth format");
        Throws<ArgumentException>(() => descriptor.depthBufferBits = 20, "20 depth bits", "20");
        Throws<ArgumentException>(() => new RenderTextureDescriptor(8, 8, RenderTextureFormat.ARGB32, 8), "8 depth bits", "8");
    }

    private static void FormatMapping()
    {
        var pairs = new (RenderTextureFormat Format, GraphicsFormat Graphics)[]
        {
            (RenderTextureFormat.ARGBHalf, GraphicsFormat.R16G16B16A16_SFloat), (RenderTextureFormat.ARGBFloat, GraphicsFormat.R32G32B32A32_SFloat),
            (RenderTextureFormat.RFloat, GraphicsFormat.R32_SFloat), (RenderTextureFormat.RHalf, GraphicsFormat.R16_SFloat),
            (RenderTextureFormat.RGHalf, GraphicsFormat.R16G16_SFloat), (RenderTextureFormat.RGFloat, GraphicsFormat.R32G32_SFloat),
            (RenderTextureFormat.RGB111110Float, GraphicsFormat.B10G11R11_UFloatPack32), (RenderTextureFormat.ARGB2101010, GraphicsFormat.A2R10G10B10_UNormPack32),
            (RenderTextureFormat.R16, GraphicsFormat.R16_UNorm), (RenderTextureFormat.RInt, GraphicsFormat.R32_SInt),
        };
        foreach (var (format, graphics) in pairs)
        {
            var descriptor = new RenderTextureDescriptor(4, 4, format, 0);
            Ensure(descriptor.graphicsFormat == graphics && descriptor.colorFormat == format, $"{format} maps to {graphics} and back");
        }
        var bgra = new RenderTextureDescriptor(4, 4, RenderTextureFormat.BGRA32, 0, 1, RenderTextureReadWrite.Linear);
        Ensure(bgra.graphicsFormat == GraphicsFormat.B8G8R8A8_UNorm && bgra.colorFormat == RenderTextureFormat.BGRA32, "BGRA32");
        Ensure(new RenderTextureDescriptor(4, 4, RenderTextureFormat.R8, 0).graphicsFormat == GraphicsFormat.R8_SRGB, "R8 is sRGB by default");
        Ensure(new RenderTextureDescriptor(4, 4, RenderTextureFormat.DefaultHDR, 0).graphicsFormat == GraphicsFormat.R16G16B16A16_SFloat, "DefaultHDR");

        var depth = new RenderTextureDescriptor(64, 64, RenderTextureFormat.Depth, 24);
        Ensure(depth.graphicsFormat == GraphicsFormat.None && depth.colorFormat == RenderTextureFormat.Depth && depth.depthBufferBits == 24 && !depth.sRGB, "Depth has no color and keeps its depth bits");
        var shadow = new RenderTextureDescriptor(64, 64, RenderTextureFormat.Shadowmap, 32);
        Ensure(shadow.graphicsFormat == GraphicsFormat.None && shadow.colorFormat == RenderTextureFormat.Shadowmap && shadow.shadowSamplingMode == ShadowSamplingMode.CompareDepths, "Shadowmap samples depth with comparison");

        var swap = new RenderTextureDescriptor(4, 4, RenderTextureFormat.ARGB32, 0);
        swap.sRGB = false;
        Ensure(swap.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm && !swap.sRGB && swap.colorFormat == RenderTextureFormat.ARGB32, "sRGB off picks the UNorm variant");
        swap.sRGB = true;
        Ensure(swap.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && swap.sRGB, "sRGB on picks the SRGB variant");
        swap.colorFormat = RenderTextureFormat.ARGBHalf;
        Ensure(swap.graphicsFormat == GraphicsFormat.R16G16B16A16_SFloat && swap.colorFormat == RenderTextureFormat.ARGBHalf, "setting colorFormat");
        swap.graphicsFormat = GraphicsFormat.R8G8B8A8_SRGB;
        Ensure(swap.sRGB, "an sRGB graphics format sets sRGB");
        swap.colorFormat = RenderTextureFormat.Depth;
        Ensure(swap.graphicsFormat == GraphicsFormat.None, "Depth drops the color");

        Throws<NotSupportedException>(() => new RenderTextureDescriptor(4, 4, RenderTextureFormat.ARGB1555, 0), "ARGB1555", "ARGB1555");
        Throws<NotSupportedException>(() => _ = new RenderTextureDescriptor(4, 4, GraphicsFormat.R8G8B8_UNorm, 0).colorFormat, "an unmapped color format", "R8G8B8_UNorm");
    }

    private static void DescriptorRoundTrip()
    {
        var descriptor = new RenderTextureDescriptor(320, 200, RenderTextureFormat.ARGBHalf, 32, 3, RenderTextureReadWrite.Linear)
        {
            msaaSamples = 2, volumeDepth = 4, enableRandomWrite = true, memoryless = RenderTextureMemoryless.Depth, vrUsage = VRTextureUsage.OneEye,
            useDynamicScale = true, stencilFormat = GraphicsFormat.S8_UInt, shadowSamplingMode = ShadowSamplingMode.RawDepth, dimension = TextureDimension.Tex3D,
        };
        var texture = new RenderTexture(descriptor);
        var back = texture.descriptor;
        Ensure(back is { width: 320, height: 200, depthBufferBits: 32, msaaSamples: 2, volumeDepth: 4, mipCount: 3, enableRandomWrite: true, useDynamicScale: true, useMipMap: true }, "descriptor numbers survive");
        Ensure(back is { graphicsFormat: GraphicsFormat.R16G16B16A16_SFloat, stencilFormat: GraphicsFormat.S8_UInt, depthStencilFormat: GraphicsFormat.D32_SFloat }, "descriptor formats survive");
        Ensure(back is { memoryless: RenderTextureMemoryless.Depth, vrUsage: VRTextureUsage.OneEye, shadowSamplingMode: ShadowSamplingMode.RawDepth, dimension: TextureDimension.Tex3D } && back.flags == descriptor.flags, "descriptor settings survive");
        Ensure(texture.width == 320 && texture.height == 200 && texture.depth == 32 && texture.antiAliasing == 2 && texture.volumeDepth == 4 && texture.format == RenderTextureFormat.ARGBHalf, "the texture reports its descriptor");
        Ensure(texture.enableRandomWrite && texture.useMipMap && texture.useDynamicScale && texture.memorylessMode == RenderTextureMemoryless.Depth && texture.vrUsage == VRTextureUsage.OneEye && texture.isVolume, "settings read through the texture");
        Ensure(texture.graphicsFormat == GraphicsFormat.R16G16B16A16_SFloat && texture.stencilFormat == GraphicsFormat.S8_UInt && texture.depthStencilFormat == GraphicsFormat.D32_SFloat && texture.dimension == TextureDimension.Tex3D && !texture.sRGB, "formats read through the texture");

        var changed = back;
        changed.width = 64;
        texture.descriptor = changed;
        Ensure(texture.width == 64 && texture.descriptor.height == 200, "a descriptor can be assigned");
        Throws<ArgumentException>(() => texture.descriptor = default, "an empty descriptor", "width");
        var copy = new RenderTexture(texture) { name = "copy" };
        Ensure(copy.width == 64 && copy.format == RenderTextureFormat.ARGBHalf && copy.antiAliasing == 2 && !copy.IsCreated(), "a copy takes the description");
        Throws<ArgumentNullException>(() => new RenderTexture((RenderTexture)null!), "copying null");
        Throws<ArgumentException>(() => new RenderTexture(0, 4, 0), "a zero width", "width");
        Throws<ArgumentException>(() => new RenderTexture(4, 0, 0), "a zero height", "height");
        Throws<ArgumentException>(() => new RenderTexture(new RenderTextureDescriptor(4, 4) { msaaSamples = 3 }), "3 samples", "antiAliasing");
    }

    private static void Lifecycle()
    {
        var texture = new RenderTexture(64, 32, 24, RenderTextureFormat.ARGB32);
        Ensure(!texture.IsCreated() && texture.wrapMode == TextureWrapMode.Clamp, "a new render texture is not created and clamps");
        Ensure(texture.Create() && texture.IsCreated(), "Create");
        Throws<InvalidOperationException>(() => texture.width = 128, "resizing a created texture", "width", "Release");
        Throws<InvalidOperationException>(() => texture.format = RenderTextureFormat.ARGBHalf, "reformatting a created texture", "format");
        Throws<InvalidOperationException>(() => texture.antiAliasing = 4, "changing antiAliasing", "antiAliasing");
        Throws<InvalidOperationException>(() => texture.descriptor = new RenderTextureDescriptor(8, 8), "assigning a descriptor", "descriptor");
        texture.autoGenerateMips = false;
        Ensure(!texture.autoGenerateMips, "settings that do not touch the allocation change freely");
        texture.Release();
        Ensure(!texture.IsCreated(), "Release");
        texture.width = 128;
        Ensure(texture.width == 128 && texture.Create(), "a released texture can change and be created again");
        texture.Release();
        texture.height = 0;
        Ensure(!texture.Create() && !texture.IsCreated(), "a texture with no size cannot be created");
        texture.height = 32;
        Throws<ArgumentOutOfRangeException>(() => texture.antiAliasing = 3, "3 samples", "antiAliasing");
        texture.antiAliasing = 8;
        texture.depth = 16;
        texture.useMipMap = true;
        texture.isCubemap = true;
        Ensure(texture.antiAliasing == 8 && texture.depth == 16 && texture.useMipMap && texture.dimension == TextureDimension.Cube && texture.isCubemap, "setters on a released texture");
        texture.isCubemap = false;
        texture.isVolume = true;
        Ensure(texture.dimension == TextureDimension.Tex3D && !texture.isCubemap, "isVolume");
        Ensure(texture.isPowerOfTwo, "a 128x32 texture is a power of two");
        texture.isPowerOfTwo = false;
        Ensure(!texture.isPowerOfTwo, "isPowerOfTwo is stored");

    }

    private static void LifecycleHints()
    {
        var named = new RenderTexture(256, 128, 0) { useMipMap = true };
        Ensure(named.mipmapCount == 9 && new RenderTexture(256, 128, 0).mipmapCount == 1 && new RenderTexture(256, 128, 0, RenderTextureFormat.ARGB32, 3).mipmapCount == 3, "mip counts");
        named.autoGenerateMips = false;
        named.Create();
        var updates = named.updateCount;
        named.GenerateMips();
        Ensure(named.updateCount == updates + 1, "GenerateMips counts an update");
        using (var log = new LogRecorder())
        {
            new RenderTexture(8, 8, 0).GenerateMips();
            Ensure(log.Messages.Count == 1 && log.Messages[0].StartsWith("Error:") && log.Messages[0].Contains("GenerateMips"), "GenerateMips without mips logs an error");
        }
        named.DiscardContents();
        named.DiscardContents(true, false);
        named.MarkRestoreExpected();
        named.ApplyDynamicScale();
        named.ResolveAntiAliasedSurface();
        named.ResolveAntiAliasedSurface(new RenderTexture(8, 8, 0));
        Throws<ArgumentNullException>(() => named.ResolveAntiAliasedSurface(null!), "resolving into null");
        Ensure(named.IsCreated(), "the hints leave the texture created");
        Ensure(new RenderTexture(8, 8, 24).depthBuffer.GetNativeRenderBufferPtr() == IntPtr.Zero && named.GetNativeDepthBufferPtr() == IntPtr.Zero && named.GetTexelOffset() == Vector2.zero, "no GPU object");
        NotSupported(() => named.ConvertToEquirect(new RenderTexture(8, 8, 0)), "ConvertToEquirect", "RenderTexture.ConvertToEquirect");
        Ensure(RenderTexture.SupportsStencil(new RenderTexture(8, 8, 24)) && !RenderTexture.SupportsStencil(new RenderTexture(8, 8, 16)) && !RenderTexture.SupportsStencil(new RenderTexture(8, 8, 0)), "SupportsStencil follows the depth format");
        Throws<ArgumentNullException>(() => RenderTexture.SupportsStencil(null!), "SupportsStencil(null)");
        var global = new RenderTexture(8, 8, 0);
        global.SetGlobalShaderProperty("_RenderTextureTestGlobal");
        Ensure(ReferenceEquals(Shader.GetGlobalTexture("_RenderTextureTestGlobal"), global), "SetGlobalShaderProperty binds the texture");
        Ensure(RenderTexture.enabled, "RenderTexture.enabled starts true");
        RenderTexture.enabled = false;
        Ensure(!RenderTexture.enabled, "RenderTexture.enabled is stored");
        RenderTexture.enabled = true;
    }

    private static void ReadThroughTexture()
    {
        Texture srgb = new RenderTexture(8, 8, 0, RenderTextureFormat.ARGB32, RenderTextureReadWrite.sRGB);
        Texture linear = new RenderTexture(8, 8, 0, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear);
        Ensure(srgb.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && srgb.isDataSRGB, "a Texture-typed sRGB render texture reports its format");
        Ensure(linear.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm && !linear.isDataSRGB, "a Texture-typed linear render texture reports its format");
        ((RenderTexture)linear).graphicsFormat = GraphicsFormat.R8G8B8A8_SRGB;
        Ensure(linear.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && linear.isDataSRGB, "a changed format shows through the Texture reference");
    }

    private static void ActiveTexture()
    {
        Ensure(RenderTexture.active == null, "no texture is active at first");
        var first = new RenderTexture(16, 16, 0);
        RenderTexture.active = first;
        Ensure(ReferenceEquals(RenderTexture.active, first) && first.IsCreated(), "setting active creates the texture");
        var second = new RenderTexture(16, 16, 0);
        RenderTexture.active = second;
        Ensure(ReferenceEquals(RenderTexture.active, second) && second.IsCreated() && first.IsCreated(), "active changes without releasing the previous texture");
        second.Release();
        Ensure(RenderTexture.active == null, "releasing the active texture clears it");
        RenderTexture.active = first;
        RenderTexture.active = null;
        Ensure(RenderTexture.active == null && first.IsCreated(), "active can be cleared");
    }

    private static void Temporaries()
    {
        var descriptor = new RenderTextureDescriptor(200, 100, RenderTextureFormat.ARGB32, 24);
        var a = RenderTexture.GetTemporary(descriptor);
        Ensure(a.IsCreated() && a.width == 200 && a.depth == 24 && a.name.StartsWith("TempBuffer") && a.hideFlags == HideFlags.HideAndDontSave, "a temporary texture is created and hidden");
        var b = RenderTexture.GetTemporary(descriptor);
        Ensure(!ReferenceEquals(a, b), "an outstanding texture is not handed out twice");
        RenderTexture.ReleaseTemporary(a);
        var other = RenderTexture.GetTemporary(new RenderTextureDescriptor(200, 101, RenderTextureFormat.ARGB32, 24));
        Ensure(!ReferenceEquals(other, a), "a different descriptor gets a different texture");
        var again = RenderTexture.GetTemporary(descriptor);
        Ensure(ReferenceEquals(again, a), "a released texture with the same descriptor is reused");
        Throws<ArgumentException>(() => RenderTexture.ReleaseTemporary(new RenderTexture(8, 8, 0)), "releasing a texture GetTemporary did not hand out", "GetTemporary");
        RenderTexture.ReleaseTemporary(again);
        Throws<ArgumentException>(() => RenderTexture.ReleaseTemporary(again), "releasing twice", "already released");
        Throws<ArgumentNullException>(() => RenderTexture.ReleaseTemporary(null!), "releasing null");
        RenderTexture.ReleaseTemporary(b);
        RenderTexture.ReleaseTemporary(other);
        Ensure(ReferenceEquals(RenderTexture.GetTemporary(descriptor), a) && ReferenceEquals(RenderTexture.GetTemporary(descriptor), b), "pooled textures come back for their descriptor, oldest first");

        var active = RenderTexture.GetTemporary(32, 32);
        RenderTexture.active = active;
        RenderTexture.ReleaseTemporary(active);
        Ensure(RenderTexture.active == null, "releasing a temporary clears active");
        Throws<ArgumentException>(() => RenderTexture.GetTemporary(0, 8), "a temporary of no size", "width");
    }

    private static void TemporaryPoolLimit()
    {
        var textures = Enumerable.Range(1, 70).Select(size => RenderTexture.GetTemporary(size, 1)).ToArray();
        foreach (var texture in textures) RenderTexture.ReleaseTemporary(texture);
        Ensure(textures.Take(6).All(texture => texture == null) && textures.Skip(6).All(texture => texture), "the pool destroys the oldest textures beyond 64");
        Ensure(textures.Take(6).All(texture => !texture.IsCreated()), "an evicted texture is released");
        var fresh = RenderTexture.GetTemporary(1, 1);
        Ensure(fresh && !ReferenceEquals(fresh, textures[0]), "an evicted descriptor gets a new texture");
        Ensure(ReferenceEquals(RenderTexture.GetTemporary(70, 1), textures[69]), "a kept texture is reused");

        var overloads = new[]
        {
            RenderTexture.GetTemporary(10, 10), RenderTexture.GetTemporary(10, 10, 16), RenderTexture.GetTemporary(10, 10, 16, RenderTextureFormat.ARGBHalf),
            RenderTexture.GetTemporary(10, 10, 16, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear),
            RenderTexture.GetTemporary(10, 10, 16, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear, 4),
            RenderTexture.GetTemporary(10, 10, 16, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear, 4, RenderTextureMemoryless.Depth),
            RenderTexture.GetTemporary(10, 10, 16, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear, 4, RenderTextureMemoryless.Depth, VRTextureUsage.OneEye),
            RenderTexture.GetTemporary(10, 10, 16, RenderTextureFormat.ARGB32, RenderTextureReadWrite.Linear, 4, RenderTextureMemoryless.Depth, VRTextureUsage.OneEye, true),
            RenderTexture.GetTemporary(10, 10, 16, GraphicsFormat.R16_SFloat), RenderTexture.GetTemporary(10, 10, 16, GraphicsFormat.R16_SFloat, 2),
            RenderTexture.GetTemporary(10, 10, 16, GraphicsFormat.R16_SFloat, 2, RenderTextureMemoryless.Color),
            RenderTexture.GetTemporary(10, 10, 16, GraphicsFormat.R16_SFloat, 2, RenderTextureMemoryless.Color, VRTextureUsage.TwoEyes),
            RenderTexture.GetTemporary(10, 10, 16, GraphicsFormat.R16_SFloat, 2, RenderTextureMemoryless.Color, VRTextureUsage.TwoEyes, true),
        };
        Ensure(overloads[0].depth == 0 && overloads[1].depth == 16 && overloads[2].format == RenderTextureFormat.ARGBHalf && !overloads[3].sRGB, "short GetTemporary overloads");
        Ensure(overloads[4].antiAliasing == 4 && overloads[5].memorylessMode == RenderTextureMemoryless.Depth && overloads[6].vrUsage == VRTextureUsage.OneEye && overloads[7].useDynamicScale, "long RenderTextureFormat overloads");
        Ensure(overloads[8].graphicsFormat == GraphicsFormat.R16_SFloat && overloads[9].antiAliasing == 2 && overloads[10].memorylessMode == RenderTextureMemoryless.Color
            && overloads[11].vrUsage == VRTextureUsage.TwoEyes && overloads[12].useDynamicScale, "GraphicsFormat overloads");
        Ensure(overloads.Select(texture => texture.GetInstanceID()).Distinct().Count() == overloads.Length, "each overload leases its own texture");
    }

    private static void Settings()
    {
        var texture = new RenderTexture(64, 64, 0, RenderTextureFormat.ARGB32, RenderTextureReadWrite.sRGB);
        Ensure(texture.sRGB && texture.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, "ReadWrite.sRGB");
        texture.format = RenderTextureFormat.ARGBHalf;
        Ensure(texture.graphicsFormat == GraphicsFormat.R16G16B16A16_SFloat && !texture.sRGB, "setting format");
        texture.graphicsFormat = GraphicsFormat.R32_SFloat;
        Ensure(texture.format == RenderTextureFormat.RFloat, "setting graphicsFormat");
        texture.stencilFormat = GraphicsFormat.S8_UInt;
        texture.depthStencilFormat = GraphicsFormat.D24_UNorm_S8_UInt;
        Ensure(texture.depth == 24 && RenderTexture.SupportsStencil(texture), "depthStencilFormat and depth");
        texture.bindTextureMS = true;
        texture.enableShadingRate = true;
        texture.useDynamicScaleExplicit = true;
        texture.vrUsage = VRTextureUsage.DeviceSpecific;
        texture.memorylessMode = RenderTextureMemoryless.MSAA;
        Ensure(texture.bindTextureMS && texture.enableShadingRate && texture.useDynamicScaleExplicit && texture.vrUsage == VRTextureUsage.DeviceSpecific && texture.memorylessMode == RenderTextureMemoryless.MSAA, "stored settings");
        texture.volumeDepth = 2;
        texture.dimension = TextureDimension.Tex2DArray;
        Ensure(texture.volumeDepth == 2 && texture.dimension == TextureDimension.Tex2DArray, "array settings");
        Ensure(new RenderTexture(8, 8, 0, DefaultFormat.LDR).graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && new RenderTexture(8, 8, 0, DefaultFormat.HDR).graphicsFormat == GraphicsFormat.R16G16B16A16_SFloat, "DefaultFormat");
        var shadow = new RenderTexture(8, 8, 32, DefaultFormat.Shadow);
        Ensure(shadow.graphicsFormat == GraphicsFormat.None && shadow.format == RenderTextureFormat.Shadowmap && shadow.depth == 32, "DefaultFormat.Shadow");
        Ensure(new RenderTexture(8, 8, 24, GraphicsFormat.R8G8B8A8_UNorm) is { graphicsFormat: GraphicsFormat.R8G8B8A8_UNorm, depth: 24 }, "a GraphicsFormat constructor");
        Ensure(new RenderTexture(8, 8, 0, GraphicsFormat.R8_UNorm, 4) is { graphicsFormat: GraphicsFormat.R8_UNorm, useMipMap: true, mipmapCount: 4 }, "a GraphicsFormat constructor with mips");
        Ensure(new RenderTexture(8, 8, GraphicsFormat.R8_UNorm, GraphicsFormat.D32_SFloat) is { depth: 32 }, "color and depth format constructor");
        Ensure(new RenderTexture(8, 8, GraphicsFormat.R8_UNorm, GraphicsFormat.D16_UNorm, 2) is { depth: 16, useMipMap: true }, "color and depth format constructor with mips");
        Ensure(new RenderTexture(8, 8, 0, RenderTextureFormat.RHalf) is { format: RenderTextureFormat.RHalf }, "a RenderTextureFormat constructor");
        var cloned = (RenderTexture)UnityEngine.Object.Instantiate((UnityEngine.Object)new RenderTexture(8, 8, 0) { name = "rt", filterMode = FilterMode.Point });
        Ensure(cloned.name == "rt(Clone)" && cloned.filterMode == FilterMode.Point && cloned.width == 8, "a render texture clone");
    }

    private static void Identifiers()
    {
        var texture = new Texture2D(2, 2);
        var target = new RenderTexture(8, 8, 0);
        Ensure(new RenderTargetIdentifier(texture) == new RenderTargetIdentifier(texture) && new RenderTargetIdentifier(texture).Equals((object)new RenderTargetIdentifier(texture)), "the same texture is equal");
        Ensure(new RenderTargetIdentifier(texture) == (RenderTargetIdentifier)texture && (RenderTargetIdentifier)target == new RenderTargetIdentifier(target), "implicit conversion from a texture");
        Ensure(new RenderTargetIdentifier(target, 0, CubemapFace.Unknown, 0) != new RenderTargetIdentifier(target, 1, CubemapFace.Unknown, 0), "a different mip level is not equal");
        Ensure(new RenderTargetIdentifier(target, 0, CubemapFace.PositiveX, 0) != new RenderTargetIdentifier(target, 0, CubemapFace.NegativeX, 0), "a different cube face is not equal");
        Ensure(new RenderTargetIdentifier(target, 0, CubemapFace.Unknown, 0) != new RenderTargetIdentifier(target, 0, CubemapFace.Unknown, RenderTargetIdentifier.AllDepthSlices), "a different depth slice is not equal");
        Ensure(new RenderTargetIdentifier(target) != new RenderTargetIdentifier(texture) && new RenderTargetIdentifier(target) != new RenderTargetIdentifier(new RenderTexture(8, 8, 0)), "different textures are not equal");
        Ensure(new RenderTargetIdentifier("_Target") == new RenderTargetIdentifier(Shader.PropertyToID("_Target")) && (RenderTargetIdentifier)"_Target" == (RenderTargetIdentifier)Shader.PropertyToID("_Target"), "a name and its ID are equal");
        Ensure(new RenderTargetIdentifier("_Target") != new RenderTargetIdentifier("_Other"), "different names are not equal");
        Ensure(new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget) == (RenderTargetIdentifier)BuiltinRenderTextureType.CameraTarget
            && new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget) != new RenderTargetIdentifier(BuiltinRenderTextureType.Depth), "built-in targets");
        Ensure(new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget).GetHashCode() == new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget).GetHashCode(), "equal identifiers hash alike");
        Ensure(new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget).GetHashCode() != new RenderTargetIdentifier(BuiltinRenderTextureType.Depth).GetHashCode(), "different identifiers usually hash differently");
        Ensure(!new RenderTargetIdentifier(texture).Equals("text") && !new RenderTargetIdentifier(texture).Equals((object?)null), "Equals(object) of another type");

        Ensure(new RenderTargetIdentifier(BuiltinRenderTextureType.CameraTarget).ToString() == "Type CameraTarget NameID -1 InstanceID 0", "ToString of a built-in target");
        var named = new RenderTargetIdentifier("_Target");
        Ensure(named.ToString() == $"Type PropertyName NameID {Shader.PropertyToID("_Target")} InstanceID 0", "ToString of a property name");
        Ensure(new RenderTargetIdentifier(target).ToString() == $"Type RenderTexture NameID -1 InstanceID {target.GetInstanceID()}", "ToString of a render texture");
        Ensure(new RenderTargetIdentifier(texture).ToString() == $"Type BindableTexture NameID -1 InstanceID {texture.GetInstanceID()}", "ToString of another texture");
        Ensure(new RenderTargetIdentifier((Texture)null!).ToString() == "Type None NameID -1 InstanceID 0", "ToString of no texture");
        Ensure(RenderTargetIdentifier.Invalid != default && RenderTargetIdentifier.Invalid == new RenderTargetIdentifier(BuiltinRenderTextureType.None, -1, CubemapFace.Unknown, -1)
            && RenderTargetIdentifier.AllDepthSlices == -1, "Invalid and AllDepthSlices");

        var color = target.colorBuffer;
        var depth = target.depthBuffer;
        var colorId = new RenderTargetIdentifier(color, 0, CubemapFace.Unknown, 0);
        Ensure(colorId.ToString().StartsWith("Type BufferPtr") && colorId != new RenderTargetIdentifier(depth, 0, CubemapFace.Unknown, 0) && colorId == (RenderTargetIdentifier)color, "buffers of one texture differ and convert");
        Ensure(colorId == new RenderTargetIdentifier(target.colorBuffer, 0, CubemapFace.Unknown, 0), "the same buffer is equal");
        var slice = new RenderTargetIdentifier(named, 2, CubemapFace.PositiveY, 3);
        Ensure(slice.ToString() == named.ToString() && slice != named && slice == new RenderTargetIdentifier(named, 2, CubemapFace.PositiveY, 3), "overriding the mip, face and slice of an identifier");
        Ensure(new RenderTargetIdentifier(7).ToString() == "Type PropertyName NameID 7 InstanceID 0" && new RenderTargetIdentifier(7, 1, CubemapFace.Unknown, 0) != new RenderTargetIdentifier(7), "a bare name ID");
    }
}
