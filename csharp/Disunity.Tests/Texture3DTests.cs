using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Texture3D: construction and validation, per-voxel pixel access, mip regeneration, readability, formats that
/// keep bytes only, and cloning.</summary>
internal static class Texture3DTests
{
    public static void Run()
    {
        ConstructorsDescribeTheVolume();
        ConstructorsValidateSizeAndMips();
        VoxelsHoldTheirOwnPixels();
        WholeLevelsListXThenYThenZ();
        ApplyRegeneratesMipsFromEightVoxels();
        ReadabilityAndBounds();
        FormatsWithoutCodecsKeepBytesOnly();
        CloneCopiesPixelsAndSampler();
        Console.WriteLine("PASS: Texture3D construction, per-voxel pixels, mips, readability, byte-only formats and clones");
    }

    private static Color Shade(float value) => new(value, value / 2, value / 4, 1f);

    private static void ConstructorsDescribeTheVolume()
    {
        var plain = new Texture3D(4, 2, 3, TextureFormat.RGBA32, false);
        Expect.That(plain.width == 4 && plain.height == 2 && plain.depth == 3, "the volume has its size");
        Expect.That(plain.format == TextureFormat.RGBA32 && plain.mipmapCount == 1 && plain.isReadable, "one readable level of its format");
        Expect.That(plain.dimension == TextureDimension.Tex3D, "the dimension is 3D");
        Expect.That(plain.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, $"a color volume is sRGB by default, got {plain.graphicsFormat}");
        var chain = new Texture3D(8, 4, 2, TextureFormat.RGBA32, true);
        Expect.Equal(4, chain.mipmapCount, "a chain runs down to 1x1x1 from the largest side");
        var deep = new Texture3D(2, 2, 16, TextureFormat.RGBA32, true);
        Expect.Equal(5, deep.mipmapCount, "the depth counts toward the chain length");
        Expect.Equal(2, new Texture3D(8, 8, 8, TextureFormat.RGBA32, 2).mipmapCount, "an explicit mip count");
        Expect.Equal(2, new Texture3D(2, 2, 1, TextureFormat.RGBA32, 9).mipmapCount, "a mip count above the full chain is capped");
        Expect.Equal(3, new Texture3D(4, 4, 4, GraphicsFormat.R8G8B8A8_UNorm, TextureCreationFlags.MipChain).mipmapCount, "a GraphicsFormat volume with a mip chain flag");
        Expect.That(new Texture3D(4, 4, 4, DefaultFormat.LDR, TextureCreationFlags.None).mipmapCount == 1, "a default format volume");
        Expect.That(new Texture3D(4, 4, 4, TextureFormat.RGBA32, true, true).mipmapCount == 3, "the createUninitialized overload");
    }

    private static void ConstructorsValidateSizeAndMips()
    {
        Throws<ArgumentException>(() => new Texture3D(4, 4, 0, TextureFormat.RGBA32, false), "zero depth", "size");
        Throws<ArgumentException>(() => new Texture3D(0, 4, 1, TextureFormat.RGBA32, false), "zero width", "size");
        Throws<ArgumentException>(() => new Texture3D(4, -1, 1, TextureFormat.RGBA32, false), "negative height", "size");
        Throws<ArgumentException>(() => new Texture3D(4, 4, 99999, TextureFormat.RGBA32, false), "a huge depth", "size");
        Throws<ArgumentException>(() => new Texture3D(4, 4, 1, TextureFormat.RGBA32, 0), "no mip levels", "mipCount");
        Throws<NotSupportedException>(() => new Texture3D(4, 4, 1, TextureFormat.DXT1Crunched, false), "a crunched format", "DXT1Crunched");
        Throws<NotSupportedException>(() => new Texture3D(2048, 2048, 2048, TextureFormat.RGBAFloat, false), "more bytes than an array holds", "too large");
    }

    private static void VoxelsHoldTheirOwnPixels()
    {
        var volume = new Texture3D(2, 2, 2, TextureFormat.RGBAFloat, false);
        volume.SetPixel(1, 0, 1, Shade(1));
        Near(Shade(1), volume.GetPixel(1, 0, 1), 1e-6f, "a voxel reads back what was set");
        Near(default, volume.GetPixel(0, 0, 1), 0f, "its neighbour in x stays empty");
        Near(default, volume.GetPixel(1, 0, 0), 0f, "its neighbour in z stays empty");
        Near(default, volume.GetPixel(1, 1, 1), 0f, "its neighbour in y stays empty");
        volume.SetPixel(0, 0, 0, Shade(0.5f), 0);
        Near(Shade(0.5f), volume.GetPixel(0, 0, 0, 0), 1e-6f, "the level overloads");
    }

    private static void WholeLevelsListXThenYThenZ()
    {
        var volume = new Texture3D(2, 2, 2, TextureFormat.RGBAFloat, false);
        var colors = Enumerable.Range(0, 8).Select(i => Shade(i / 8f)).ToArray();
        volume.SetPixels(colors);
        Near(Shade(1 / 8f), volume.GetPixel(1, 0, 0), 1e-6f, "x runs fastest");
        Near(Shade(2 / 8f), volume.GetPixel(0, 1, 0), 1e-6f, "then y");
        Near(Shade(4 / 8f), volume.GetPixel(0, 0, 1), 1e-6f, "then z");
        var read = volume.GetPixels();
        Expect.Equal(8, read.Length, "a level has width x height x depth colors");
        Near(Shade(7 / 8f), read[7], 1e-6f, "the last color is the far top right");
        volume.SetPixels32([new Color32(255, 0, 0, 255), default, default, default, default, default, default, new Color32(0, 0, 255, 255)]);
        var packed = volume.GetPixels32();
        Expect.That(packed[0].r == 255 && packed[7].b == 255, "SetPixels32 and GetPixels32 round trip");
        Throws<ArgumentException>(() => volume.SetPixels([Shade(1)]), "too few colors", "colors");
        Throws<ArgumentNullException>(() => volume.SetPixels(null!), "no colors", "colors");
    }

    private static void ApplyRegeneratesMipsFromEightVoxels()
    {
        var volume = new Texture3D(2, 2, 2, TextureFormat.RGBAFloat, true);
        var before = volume.updateCount;
        volume.SetPixels([Shade(1), Shade(0), Shade(1), Shade(0), Shade(1), Shade(0), Shade(1), Shade(0)]);
        volume.Apply();
        Near(Shade(0.5f), volume.GetPixels(1)[0], 1e-6f, "the 1x1x1 level averages all eight voxels");
        Expect.That(volume.updateCount == before + 1, "Apply counts one update");
        volume.SetPixels([Shade(0.75f)], 1);
        volume.Apply(false);
        Near(Shade(0.75f), volume.GetPixels(1)[0], 1e-6f, "Apply(false) keeps a level that was written");
        var flat = new Texture3D(4, 1, 1, TextureFormat.RGBAFloat, true);
        flat.SetPixels([Shade(1), Shade(0), Shade(0.5f), Shade(0.5f)]);
        flat.Apply();
        Near(Shade(0.5f), flat.GetPixels(1)[0], 1e-6f, "a side of one clamps instead of reading past the level");
        Near(Shade(0.5f), flat.GetPixels(1)[1], 1e-6f, "and the next voxel averages its own pair");
    }

    private static void ReadabilityAndBounds()
    {
        var volume = new Texture3D(2, 2, 2, TextureFormat.RGBA32, false);
        Throws<ArgumentException>(() => volume.GetPixel(2, 0, 0), "x past the width", "voxel");
        Throws<ArgumentException>(() => volume.GetPixel(0, 2, 0), "y past the height", "voxel");
        Throws<ArgumentException>(() => volume.GetPixel(0, 0, 2), "z past the depth", "voxel");
        Throws<ArgumentException>(() => volume.SetPixel(0, 0, -1, default), "a negative z", "voxel");
        Throws<ArgumentException>(() => volume.GetPixels(1), "a mip level past the chain", "mip level");
        Throws<ArgumentException>(() => volume.GetPixel(0, 0, 0, -1), "a negative mip level", "mip level");
        volume.Apply(false, true);
        Expect.That(!volume.isReadable, "Apply can make the volume no longer readable");
        Throws<InvalidOperationException>(() => volume.GetPixels(), "reading an unreadable volume", "not readable");
        Throws<InvalidOperationException>(() => volume.SetPixel(0, 0, 0, default), "writing an unreadable volume", "not readable");
        Throws<InvalidOperationException>(() => volume.Apply(), "applying an unreadable volume", "not readable");
    }

    private static void FormatsWithoutCodecsKeepBytesOnly()
    {
        var packed = new Texture3D(8, 8, 4, TextureFormat.DXT1, true);
        Expect.Equal(4, packed.mipmapCount, "a block format volume has a full mip chain");
        NotSupported(() => packed.GetPixels(), "reading a block format", "Texture3D.GetPixels");
        NotSupported(() => packed.SetPixel(0, 0, 0, default), "writing a block format", "Texture3D.SetPixel");
        packed.Apply();
        Expect.Equal(1u, packed.updateCount, "Apply on a block format counts an update and leaves the bytes");
    }

    private static void CloneCopiesPixelsAndSampler()
    {
        var volume = new Texture3D(2, 1, 2, TextureFormat.RGBA32, false) { name = "Original", filterMode = FilterMode.Point, anisoLevel = 4 };
        volume.SetPixel(1, 0, 1, Shade(1));
        volume.Apply();
        var copy = UnityEngine.Object.Instantiate(volume);
        Expect.That(!ReferenceEquals(copy, volume) && copy.depth == 2 && copy.width == 2, "the clone has the size of the original");
        Near(volume.GetPixel(1, 0, 1), copy.GetPixel(1, 0, 1), 0.003f, "the clone has the original's pixels");
        Expect.That(copy.filterMode == FilterMode.Point && copy.anisoLevel == 4, "the clone keeps the sampler");
        copy.SetPixel(1, 0, 1, Shade(0));
        Near(Shade(1), volume.GetPixel(1, 0, 1), 0.003f, "writing the clone leaves the original");
    }
}
