using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Texture2DArray: construction and validation, per-slice pixel access, mip regeneration, readability, formats that
/// keep bytes only, and cloning.</summary>
internal static class Texture2DArrayTests
{
    public static void Run()
    {
        ConstructorsDescribeTheArray();
        ConstructorsValidateSizeDepthAndMips();
        SlicesHoldTheirOwnPixels();
        ApplyRegeneratesEverySlicesMips();
        ReadabilityAndBounds();
        FormatsWithoutCodecsKeepBytesOnly();
        CloneCopiesPixelsAndSampler();
        Console.WriteLine("PASS: Texture2DArray construction, per-slice pixels, mips, readability, byte-only formats and clones");
    }

    private static void ConstructorsDescribeTheArray()
    {
        var plain = new Texture2DArray(4, 2, 3, TextureFormat.RGBA32, false);
        Expect.That(plain.width == 4 && plain.height == 2 && plain.depth == 3, "the array has its size and slice count");
        Expect.That(plain.format == TextureFormat.RGBA32 && plain.mipmapCount == 1 && plain.isReadable, "one readable level of its format");
        Expect.That(plain.dimension == TextureDimension.Tex2DArray, "the dimension is a 2D array");
        Expect.That(plain.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, $"a color array is sRGB by default, got {plain.graphicsFormat}");
        var chain = new Texture2DArray(8, 4, 2, TextureFormat.RGBA32, true, true);
        Expect.That(chain.mipmapCount == 4 && chain.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm, "a linear mip chain runs 8x4 down to 1x1");
        var counted = new Texture2DArray(8, 8, 1, TextureFormat.RGBA32, 2, false);
        Expect.Equal(2, counted.mipmapCount, "an explicit mip count");
        var capped = new Texture2DArray(2, 2, 1, TextureFormat.RGBA32, 9, false);
        Expect.Equal(2, capped.mipmapCount, "a mip count above the full chain is capped");
        var graphics = new Texture2DArray(4, 4, 2, GraphicsFormat.R8G8B8A8_UNorm, TextureCreationFlags.MipChain);
        Expect.That(graphics.format == TextureFormat.RGBA32 && graphics.mipmapCount == 3, "a GraphicsFormat array with a mip chain flag");
        var defaulted = new Texture2DArray(4, 4, 2, DefaultFormat.LDR, TextureCreationFlags.None);
        Expect.That(defaulted.mipmapCount == 1 && defaulted.depth == 2, "a default format array");
    }

    private static void ConstructorsValidateSizeDepthAndMips()
    {
        Throws<ArgumentException>(() => new Texture2DArray(4, 4, 0, TextureFormat.RGBA32, false), "zero slices", "depth");
        Throws<ArgumentException>(() => new Texture2DArray(4, 4, -1, TextureFormat.RGBA32, false), "negative slices", "depth");
        Throws<ArgumentException>(() => new Texture2DArray(0, 4, 1, TextureFormat.RGBA32, false), "zero width", "size");
        Throws<ArgumentException>(() => new Texture2DArray(4, 99999, 1, TextureFormat.RGBA32, false), "a huge height", "size");
        Throws<ArgumentException>(() => new Texture2DArray(4, 4, 99999, TextureFormat.RGBA32, false), "too many slices", "depth");
        Throws<ArgumentException>(() => new Texture2DArray(4, 4, 1, TextureFormat.RGBA32, 0, false), "no mip levels", "mipCount");
        Throws<NotSupportedException>(() => new Texture2DArray(4, 4, 1, TextureFormat.DXT1Crunched, false), "a crunched format", "DXT1Crunched");
        Throws<NotSupportedException>(() => new Texture2DArray(16384, 16384, 2048, TextureFormat.RGBAFloat, false), "more bytes than an array holds", "too large");
    }

    private static Color Shade(float value) => new(value, value / 2, value / 4, 1f);

    private static void SlicesHoldTheirOwnPixels()
    {
        var array = new Texture2DArray(2, 2, 3, TextureFormat.RGBAFloat, false);
        array.SetPixels([Shade(1), Shade(0.5f), Shade(0.25f), Shade(0.125f)], 1);
        Near(default, array.GetPixels(0)[0], 0f, "slice 0 stays empty");
        Near(default, array.GetPixels(2)[3], 0f, "slice 2 stays empty");
        var read = array.GetPixels(1);
        Expect.Equal(4, read.Length, "a slice has width x height colors");
        Near(Shade(0.5f), read[1], 1e-6f, "colors read back in row order from the bottom left");
        Near(Shade(0.125f), read[3], 1e-6f, "the last color is the top right");
        array.SetPixels32([new Color32(255, 0, 0, 255), default, default, new Color32(0, 0, 255, 255)], 2);
        var packed = array.GetPixels32(2);
        Expect.That(packed[0].r == 255 && packed[3].b == 255, "SetPixels32 and GetPixels32 round trip");
        Throws<ArgumentException>(() => array.SetPixels([Shade(1)], 0), "too few colors", "colors");
        Throws<ArgumentNullException>(() => array.SetPixels(null!, 0), "no colors", "colors");
    }

    private static void ApplyRegeneratesEverySlicesMips()
    {
        var array = new Texture2DArray(2, 2, 2, TextureFormat.RGBAFloat, true);
        var before = array.updateCount;
        array.SetPixels([Shade(1), Shade(0), Shade(1), Shade(0)], 0);
        array.SetPixels([Shade(0.5f), Shade(0.5f), Shade(0.5f), Shade(0.5f)], 1);
        array.Apply();
        Near(Shade(0.5f), array.GetPixels(0, 1)[0], 1e-6f, "slice 0's 1x1 level averages its four texels");
        Near(Shade(0.5f), array.GetPixels(1, 1)[0], 1e-6f, "slice 1's 1x1 level is its own average");
        Expect.That(array.updateCount == before + 1, "Apply counts one update");
        array.SetPixels([Shade(0.75f)], 0, 1);
        array.Apply(false);
        Near(Shade(0.75f), array.GetPixels(0, 1)[0], 1e-6f, "Apply(false) keeps a level that was written");
    }

    private static void ReadabilityAndBounds()
    {
        var array = new Texture2DArray(2, 2, 2, TextureFormat.RGBA32, false);
        Throws<ArgumentException>(() => array.GetPixels(2), "an element past the depth", "array element");
        Throws<ArgumentException>(() => array.GetPixels(-1), "a negative element", "array element");
        Throws<ArgumentException>(() => array.GetPixels(0, 1), "a mip level past the chain", "mip level");
        Throws<ArgumentException>(() => array.SetPixels(new Color[4], 2, 0), "writing past the depth", "array element");
        array.Apply(false, true);
        Expect.That(!array.isReadable, "Apply can make the array no longer readable");
        Throws<InvalidOperationException>(() => array.GetPixels(0), "reading an unreadable array", "not readable");
        Throws<InvalidOperationException>(() => array.SetPixels(new Color[4], 0), "writing an unreadable array", "not readable");
        Throws<InvalidOperationException>(() => array.Apply(), "applying an unreadable array", "not readable");
    }

    private static void FormatsWithoutCodecsKeepBytesOnly()
    {
        var packed = new Texture2DArray(8, 8, 2, TextureFormat.DXT1, true);
        Expect.Equal(4, packed.mipmapCount, "a block format array has a full mip chain");
        NotSupported(() => packed.GetPixels(0), "reading a block format", "Texture2DArray.GetPixels");
        NotSupported(() => packed.SetPixels(new Color[64], 0), "writing a block format", "Texture2DArray.SetPixels");
        packed.Apply();
        Expect.Equal(1u, packed.updateCount, "Apply on a block format counts an update and leaves the bytes");
    }

    private static void CloneCopiesPixelsAndSampler()
    {
        var array = new Texture2DArray(2, 1, 2, TextureFormat.RGBA32, false) { name = "Original", filterMode = FilterMode.Point, anisoLevel = 4 };
        array.SetPixels([Shade(1), Shade(0.5f)], 1);
        array.Apply();
        var copy = UnityEngine.Object.Instantiate(array);
        Expect.That(!ReferenceEquals(copy, array) && copy.depth == 2 && copy.width == 2, "the clone has the size of the original");
        Near(array.GetPixels(1)[1], copy.GetPixels(1)[1], 0.003f, "the clone has the original's pixels");
        Expect.That(copy.filterMode == FilterMode.Point && copy.anisoLevel == 4, "the clone keeps the sampler");
        copy.SetPixels([Shade(0), Shade(0)], 1);
        Near(Shade(0.5f), array.GetPixels(1)[1], 0.003f, "writing the clone leaves the original");
    }
}
