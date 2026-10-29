using Unity.Collections;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using static GraphicsTestSupport;

/// <summary>Texture and Texture2D: pixel storage and access, mip levels, raw data, built-in textures and settings.</summary>
internal static class TextureTests
{
    public static void Run()
    {
        PixelRoundTrips();
        ViewsSurviveReinitialize();
        RawLayouts();
        PackedChannelOrder();
        WideChannelOrder();
        MipLevels();
        Bilinear();
        BlocksAndWrapping();
        RawDataAndViews();
        BuiltInTextures();
        CompressedFormats();
        Readability();
        FormatsAndConstructors();
        SettingsAndStatics();
        UnsupportedMembers();
        Console.WriteLine("PASS: Texture2D pixels, mip levels, raw data views, built-in textures, compressed formats and texture settings");
    }

    private static void PixelRoundTrips()
    {
        var color = new Color(1f, 0.5019608f, 0.2509804f, 0.7529412f);
        foreach (var format in new[] { TextureFormat.RGBA32, TextureFormat.ARGB32, TextureFormat.BGRA32 })
        {
            var texture = new Texture2D(4, 3, format, false);
            texture.SetPixel(2, 1, color);
            texture.Apply();
            Near(color, texture.GetPixel(2, 1), 0.003f, $"{format} round trip");
            Near(default, texture.GetPixel(0, 0), 0f, $"{format} untouched pixel");
        }
        var rgb = new Texture2D(2, 2, TextureFormat.RGB24, false);
        rgb.SetPixel(1, 0, color);
        Near(new Color(color.r, color.g, color.b, 1f), rgb.GetPixel(1, 0), 0.003f, "RGB24 keeps color and reads alpha 1");
        var single = new Texture2D(2, 2, TextureFormat.RFloat, false);
        single.SetPixel(0, 1, new Color(0.123456f, 9f, 9f, 9f));
        Ensure(single.GetPixel(0, 1).r == 0.123456f && single.GetPixel(0, 1).g == 0f && single.GetPixel(0, 1).a == 1f, "RFloat stores one exact float");
        var red8 = new Texture2D(2, 2, TextureFormat.R8, false);
        red8.SetPixel(1, 1, new Color(1f, 1f, 1f, 1f) * 0.2f);
        Ensure(MathF.Abs(red8.GetPixel(1, 1).r - 0.2f) < 0.003f, "R8 round trip");
        var half = new Texture2D(1, 1, TextureFormat.RGBAHalf, false);
        half.SetPixel(0, 0, new Color(0.5f, 2f, -1f, 0.25f));
        Near(new Color(0.5f, 2f, -1f, 0.25f), half.GetPixel(0, 0), 0f, "RGBAHalf holds values above 1 and below 0");
        var packed = new Texture2D(1, 1, TextureFormat.RGB565, false);
        packed.SetPixel(0, 0, Color.green);
        Near(Color.green, packed.GetPixel(0, 0), 0f, "RGB565");
        var alpha = new Texture2D(1, 1, TextureFormat.Alpha8, false);
        alpha.SetPixel(0, 0, new Color(0.3f, 0.3f, 0.3f, 1f));
        Ensure(MathF.Abs(alpha.GetPixel(0, 0).a - 1f) < 0.003f, "Alpha8 stores alpha");
        var low = new Texture2D(1, 1, TextureFormat.RGBA32, false);
        low.SetPixel(0, 0, new Color(2f, -1f, 0.5f, 1f));
        Near(new Color(1f, 0f, 0.5019608f, 1f), low.GetPixel(0, 0), 0.003f, "8-bit channels clamp");
        OutOfRangeWrites();
    }

    private static void ViewsSurviveReinitialize()
    {
        var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
        texture.SetPixel(0, 0, Color.red);
        var view = texture.GetRawTextureData<byte>();
        Ensure(view.Length == 16 && view[0] == 255, "the view shows the pixels");
        texture.Reinitialize(4, 4);
        var allocations = new List<byte[]>();
        for (var i = 0; i < 64; i++) allocations.Add(GC.AllocateArray<byte>(64, pinned: true));
        GC.Collect();
        GC.KeepAlive(allocations);
        Ensure(view[0] == 255 && view.Length == 16, "a view taken before Reinitialize still reads the bytes it was taken from");
        Ensure(texture.GetRawTextureData<byte>().Length == 64, "a new view has the new size");
        UnityEngine.Object.Destroy(texture);
    }

    private static byte[] RawAfterSet(TextureFormat format, Color color)
    {
        var texture = new Texture2D(1, 1, format, false);
        texture.SetPixel(0, 0, color);
        return texture.GetRawTextureData();
    }

    private static void PackedChannelOrder()
    {
        Ensure(RawAfterSet(TextureFormat.RGB565, Color.red).SequenceEqual(new byte[] { 0x00, 0xF8 }), "RGB565 keeps red in the top five bits");
        Ensure(RawAfterSet(TextureFormat.RGB565, Color.green).SequenceEqual(new byte[] { 0xE0, 0x07 }), "RGB565 keeps green in the middle six bits");
        Ensure(RawAfterSet(TextureFormat.RGB565, Color.blue).SequenceEqual(new byte[] { 0x1F, 0x00 }), "RGB565 keeps blue in the low five bits");
        var texture = new Texture2D(1, 1, TextureFormat.RGB565, false);
        texture.LoadRawTextureData(new byte[] { 0x00, 0xF8 });
        Near(Color.red, texture.GetPixel(0, 0), 0f, "RGB565 red reads back");
        texture.LoadRawTextureData(new byte[] { 0x1F, 0x00 });
        Near(Color.blue, texture.GetPixel(0, 0), 0f, "RGB565 blue reads back");

        var color = new Color(1f, 0.2f, 0.6f, 0.4f);
        Ensure(RawAfterSet(TextureFormat.RGBA4444, color).SequenceEqual(new byte[] { 0x96, 0xF3 }), "RGBA4444 packs red, green, blue, alpha from the top nibble");
        Ensure(RawAfterSet(TextureFormat.ARGB4444, color).SequenceEqual(new byte[] { 0x39, 0x6F }), "ARGB4444 packs alpha, red, green, blue from the top nibble");
        foreach (var format in new[] { TextureFormat.RGBA4444, TextureFormat.ARGB4444 })
        {
            var packed = new Texture2D(1, 1, format, false);
            packed.SetPixel(0, 0, color);
            Near(new Color(15 / 15f, 3 / 15f, 9 / 15f, 6 / 15f), packed.GetPixel(0, 0), 1e-6f, $"{format} reads its channels back in order");
        }

        var alpha = new Texture2D(1, 1, TextureFormat.Alpha8, false);
        alpha.SetPixel(0, 0, new Color(0.2f, 0.4f, 0.6f, 0.5f));
        Ensure(alpha.GetRawTextureData().SequenceEqual(new byte[] { 128 }), "Alpha8 stores only alpha");
        Near(new Color(1f, 1f, 1f, 128 / 255f), alpha.GetPixel(0, 0), 1e-6f, "Alpha8 reads as white with its alpha");
    }

    private static void WideChannelOrder()
    {
        Ensure(RawAfterSet(TextureFormat.R16, new Color(1f, 0f, 0f, 0f)).SequenceEqual(new byte[] { 0xFF, 0xFF }), "R16 stores red");
        Ensure(RawAfterSet(TextureFormat.RG32, new Color(0f, 1f, 0f, 0f)).SequenceEqual(new byte[] { 0, 0, 0xFF, 0xFF }), "RG32 stores green after red");
        Ensure(RawAfterSet(TextureFormat.RGB48, new Color(0f, 0f, 1f, 0f)).SequenceEqual(new byte[] { 0, 0, 0, 0, 0xFF, 0xFF }), "RGB48 stores blue third");
        Ensure(RawAfterSet(TextureFormat.RGBA64, new Color(0f, 0f, 0f, 1f)).SequenceEqual(new byte[] { 0, 0, 0, 0, 0, 0, 0xFF, 0xFF }), "RGBA64 stores alpha fourth");
        Ensure(RawAfterSet(TextureFormat.RHalf, new Color(2f, 0f, 0f, 0f)).SequenceEqual(new byte[] { 0x00, 0x40 }), "RHalf stores red as a half float");
        Ensure(RawAfterSet(TextureFormat.RGHalf, new Color(0f, 2f, 0f, 0f)).SequenceEqual(new byte[] { 0, 0, 0x00, 0x40 }), "RGHalf stores green after red");
        var color = new Color(0.25f, 0.5f, 0.75f, 1f);
        foreach (var (format, expected) in new[]
        {
            (TextureFormat.R16, new Color(0.25f, 0f, 0f, 1f)), (TextureFormat.RG32, new Color(0.25f, 0.5f, 0f, 1f)),
            (TextureFormat.RGB48, new Color(0.25f, 0.5f, 0.75f, 1f)), (TextureFormat.RGBA64, color),
            (TextureFormat.RHalf, new Color(0.25f, 0f, 0f, 1f)), (TextureFormat.RGHalf, new Color(0.25f, 0.5f, 0f, 1f)),
        })
        {
            var texture = new Texture2D(1, 1, format, false);
            texture.SetPixel(0, 0, color);
            Near(expected, texture.GetPixel(0, 0), 2e-5f, $"{format} keeps its channels and reads the rest as 0 and alpha as 1");
        }
    }

    private static void OutOfRangeWrites()
    {
        var clamped = new Texture2D(2, 2, TextureFormat.RGBA32, false) { wrapMode = TextureWrapMode.Clamp };
        clamped.SetPixel(5, 5, Color.red);
        clamped.SetPixel(-1, 0, Color.blue);
        Near(Color.red, clamped.GetPixel(1, 1), 0f, "Clamp writes outside the texture land on the nearest edge texel");
        Near(Color.blue, clamped.GetPixel(0, 0), 0f, "Clamp writes below zero land on the first texel");
        var repeated = new Texture2D(2, 2, TextureFormat.RGBA32, false) { wrapMode = TextureWrapMode.Repeat };
        repeated.SetPixel(3, 2, Color.green);
        Near(Color.green, repeated.GetPixel(1, 0), 0f, "Repeat writes outside the texture wrap around");
        repeated.SetPixel(-1, -1, Color.red);
        Near(Color.red, repeated.GetPixel(1, 1), 0f, "Repeat writes below zero wrap around");
    }

    private static void RawLayouts()
    {
        var color = new Color(1f, 0f, 0f, 0.5f);
        var rgba = new Texture2D(1, 1, TextureFormat.RGBA32, false);
        rgba.SetPixel(0, 0, color);
        Ensure(rgba.GetRawTextureData().SequenceEqual(new byte[] { 255, 0, 0, 128 }), "RGBA32 byte order");
        var argb = new Texture2D(1, 1, TextureFormat.ARGB32, false);
        argb.SetPixel(0, 0, color);
        Ensure(argb.GetRawTextureData().SequenceEqual(new byte[] { 128, 255, 0, 0 }), "ARGB32 byte order");
        var bgra = new Texture2D(1, 1, TextureFormat.BGRA32, false);
        bgra.SetPixel(0, 0, color);
        Ensure(bgra.GetRawTextureData().SequenceEqual(new byte[] { 0, 0, 255, 128 }), "BGRA32 byte order");
        var rows = new Texture2D(2, 2, TextureFormat.R8, false);
        rows.SetPixels32([new(10, 0, 0, 0), new(20, 0, 0, 0), new(30, 0, 0, 0), new(40, 0, 0, 0)]);
        Ensure(rows.GetRawTextureData().SequenceEqual(new byte[] { 10, 20, 30, 40 }), "pixels are stored bottom row first");
    }

    private static void MipLevels()
    {
        var chain = new Texture2D(8, 4, TextureFormat.RGBA32, true);
        Ensure(chain.mipmapCount == 4 && chain.width == 8 && chain.height == 4, "a mip chain runs down to one pixel");
        var sizes = Enumerable.Range(0, 4).Select(level => chain.GetPixelData<byte>(level).Length).ToArray();
        Ensure(sizes.SequenceEqual(new[] { 128, 32, 8, 4 }), $"mip level byte sizes: {string.Join(",", sizes)}");
        Ensure(chain.GetRawTextureData().Length == 172 && chain.GetPixels(2).Length == 2 && chain.GetPixels(3).Length == 1, "raw size and pixel counts of the levels");
        Ensure(new Texture2D(16, 16, TextureFormat.RGBA32, 3, false).mipmapCount == 3, "an explicit mip count");
        Ensure(new Texture2D(16, 16, TextureFormat.RGBA32, 100, false).mipmapCount == 5, "a larger mip count is the full chain");
        Ensure(new Texture2D(16, 16, TextureFormat.RGBA32, Texture.GenerateAllMips, false).mipmapCount == 5 && Texture.GenerateAllMips == -1, "GenerateAllMips");
        Throws<ArgumentException>(() => new Texture2D(4, 4, TextureFormat.RGBA32, 0, false), "a zero mip count");
        Throws<ArgumentException>(() => chain.GetPixels(4), "a mip level out of range", "mip level 4");
        Throws<ArgumentException>(() => new Texture2D(0, 4), "a zero width");

        var checker = new Texture2D(4, 4, TextureFormat.RGBA32, true);
        var updates = checker.updateCount;
        for (var y = 0; y < 4; y++)
            for (var x = 0; x < 4; x++) checker.SetPixel(x, y, (x + y) % 2 == 0 ? Color.white : Color.black);
        checker.SetPixel(0, 0, Color.white, 1);
        Near(Color.white, checker.GetPixel(0, 0, 1), 0f, "SetPixel on a level");
        checker.Apply(true);
        Ensure(checker.updateCount == updates + 1, "Apply counts an update");
        Near(new Color(0.5f, 0.5f, 0.5f, 1f), checker.GetPixel(0, 0, 1), 0.003f, "Apply averages the level above");
        Near(new Color(0.5f, 0.5f, 0.5f, 1f), checker.GetPixel(0, 0, 2), 0.003f, "Apply fills every level");
        checker.SetPixel(1, 1, Color.red, 1);
        checker.Apply(false);
        Near(Color.red, checker.GetPixel(1, 1, 1), 0f, "Apply(false) leaves the smaller levels alone");
        Ensure(checker.updateCount == updates + 2, "each Apply counts");
    }

    private static void Bilinear()
    {
        var texture = new Texture2D(2, 2, TextureFormat.RFloat, false);
        texture.SetPixels([new(0, 0, 0, 1), new(1, 0, 0, 1), new(1, 0, 0, 1), new(0, 0, 0, 1)]);
        Ensure(MathF.Abs(texture.GetPixelBilinear(0.5f, 0.5f).r - 0.5f) < 1e-6f, "the middle of a 2x2 is the average of its four texels");
        Ensure(texture.GetPixelBilinear(0.25f, 0.25f).r == 0f && texture.GetPixelBilinear(0.75f, 0.25f).r == 1f, "a texel center returns the texel");
        Ensure(MathF.Abs(texture.GetPixelBilinear(0.5f, 0.25f).r - 0.5f) < 1e-6f, "between two texels it is their mean");
        texture.wrapMode = TextureWrapMode.Clamp;
        Ensure(texture.GetPixelBilinear(0f, 0.25f).r == 0f, "clamp holds the edge texel");
        texture.wrapMode = TextureWrapMode.Repeat;
        Ensure(MathF.Abs(texture.GetPixelBilinear(0f, 0.25f).r - 0.5f) < 1e-6f, "repeat blends with the opposite edge");
        Ensure(texture.GetPixelBilinear(0.75f, 0.75f, 0).r == 0f, "an explicit level");
    }

    private static void BlocksAndWrapping()
    {
        var texture = new Texture2D(4, 4, TextureFormat.RGBA32, false);
        texture.SetPixels(1, 1, 2, 2, [Color.red, Color.green, Color.blue, Color.white]);
        var block = texture.GetPixels(1, 1, 2, 2);
        Ensure(block[0] == Color.red && block[1] == Color.green && block[2] == Color.blue && block[3] == Color.white, "a block reads back row by row");
        Ensure(texture.GetPixel(2, 1) == Color.green && texture.GetPixel(1, 2) == Color.blue, "block pixels land at their coordinates");
        Ensure(texture.GetPixels32(0)[1 * 4 + 1].Equals((Color32)Color.red) && texture.GetPixels32().Length == 16, "GetPixels32");
        texture.SetPixels32(0, 0, 1, 1, [new Color32(0, 255, 0, 255)]);
        texture.SetPixels32(1, 0, 1, 1, [new Color32(0, 0, 255, 255)], 0);
        Ensure(texture.GetPixel(0, 0) == Color.green && texture.GetPixel(1, 0) == Color.blue, "SetPixels32 blocks");
        Throws<ArgumentException>(() => texture.GetPixels(3, 3, 2, 2), "a block outside the level", "outside mip level");
        Throws<ArgumentException>(() => texture.SetPixels([Color.red]), "too few colors", "needs 16");
        Ensure(texture.GetPixels(0, 0, 0, 0).Length == 0, "an empty block");

        texture.SetPixels(0, 1, 4, 1, [Color.red, Color.green, Color.blue, Color.white]);
        texture.wrapMode = TextureWrapMode.Clamp;
        Ensure(texture.GetPixel(9, 1) == Color.white && texture.GetPixel(-2, 1) == Color.red, "clamp");
        texture.wrapMode = TextureWrapMode.Repeat;
        Ensure(texture.GetPixel(5, 1) == Color.green && texture.GetPixel(-1, 1) == Color.white, "repeat");
        texture.wrapMode = TextureWrapMode.Mirror;
        Ensure(texture.GetPixel(4, 1) == Color.white && texture.GetPixel(-1, 1) == Color.red && texture.GetPixel(6, 1) == Color.green, "mirror");
        texture.wrapMode = TextureWrapMode.MirrorOnce;
        Ensure(texture.GetPixel(-1, 1) == Color.red && texture.GetPixel(-2, 1) == Color.green && texture.GetPixel(9, 1) == Color.white, "mirror once");
        texture.wrapModeU = TextureWrapMode.Clamp;
        Ensure(texture.wrapModeU == TextureWrapMode.Clamp && texture.wrapModeV == TextureWrapMode.MirrorOnce && texture.wrapMode == TextureWrapMode.Clamp, "per-axis wrap");
        texture.wrapMode = TextureWrapMode.Repeat;
        Ensure(texture.wrapModeU == TextureWrapMode.Repeat && texture.wrapModeV == TextureWrapMode.Repeat && texture.wrapModeW == TextureWrapMode.Repeat, "wrapMode sets all three");
    }

    private static void RawDataAndViews()
    {
        var texture = new Texture2D(4, 4, TextureFormat.RGBA32, false);
        Ensure(texture.GetRawTextureData().Length == 64 && texture.GetRawTextureData<byte>().Length == 64 && texture.GetRawTextureData<Color32>().Length == 16, "raw data sizes");
        var view = texture.GetRawTextureData<byte>();
        view[0] = 200;
        Ensure(texture.GetRawTextureData()[0] == 200 && MathF.Abs(texture.GetPixel(0, 0).r - 200f / 255f) < 1e-6f, "a native view shares the texture's memory");
        texture.SetPixel(1, 0, new Color32(1, 2, 3, 4));
        Ensure(texture.GetPixelData<Color32>(0)[1].Equals(new Color32(1, 2, 3, 4)), "a view sees later pixel writes");
        var copy = texture.GetRawTextureData();
        copy[0] = 7;
        Ensure(texture.GetRawTextureData()[0] == 200, "the byte[] is a copy");

        var data = Enumerable.Range(0, 64).Select(i => (byte)i).ToArray();
        texture.LoadRawTextureData(data);
        Ensure(texture.GetRawTextureData().SequenceEqual(data), "LoadRawTextureData(byte[])");
        var native = new NativeArray<byte>(64, Allocator.Temp);
        native[5] = 99;
        texture.LoadRawTextureData(native);
        Ensure(texture.GetRawTextureData()[5] == 99, "LoadRawTextureData(NativeArray)");
        native.Dispose();
        unsafe
        {
            fixed (byte* pointer = data) texture.LoadRawTextureData((IntPtr)pointer, 64);
        }
        Ensure(texture.GetRawTextureData()[63] == 63, "LoadRawTextureData(IntPtr, int)");
        Throws<ArgumentException>(() => texture.LoadRawTextureData(new byte[63]), "short raw data", "63 bytes given", "holds 64");
        Throws<ArgumentException>(() => texture.LoadRawTextureData(new byte[65]), "long raw data");
        var shifted = Enumerable.Range(0, 68).Select(i => (byte)(i < 4 ? 9 : i - 3)).ToArray();
        texture.SetPixelData(shifted, 0, 4);
        Ensure(texture.GetRawTextureData()[0] == 1 && texture.GetRawTextureData()[63] == 64, "SetPixelData starts at the given element");
        Throws<ArgumentException>(() => texture.SetPixelData(new byte[10], 0), "too little pixel data", "needs 64 bytes");
    }

    private static void BuiltInTextures()
    {
        foreach (var (texture, color) in new[]
        {
            (Texture2D.whiteTexture, new Color32(255, 255, 255, 255)), (Texture2D.blackTexture, new Color32(0, 0, 0, 255)),
            (Texture2D.redTexture, new Color32(255, 0, 0, 255)), (Texture2D.grayTexture, new Color32(128, 128, 128, 255)),
            (Texture2D.linearGrayTexture, new Color32(128, 128, 128, 255)), (Texture2D.normalTexture, new Color32(128, 128, 255, 128)),
        })
        {
            Ensure(texture.width == 4 && texture.height == 4 && texture.format == TextureFormat.RGBA32, $"{texture.name} is a 4x4 RGBA32 texture");
            var bytes = (byte[])typeof(Texture2D).GetField("_data", System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Instance)!.GetValue(texture)!;
            Ensure(bytes.Length == 64 && Enumerable.Range(0, 16).All(i => bytes[4 * i] == color.r && bytes[4 * i + 1] == color.g && bytes[4 * i + 2] == color.b && bytes[4 * i + 3] == color.a), $"{texture.name} is one color");
            Ensure(!texture.isReadable, $"{texture.name} is not readable, so a game cannot change it");
            Throws<InvalidOperationException>(() => texture.SetPixel(0, 0, Color.magenta), $"{texture.name} SetPixel", "not readable");
            Throws<InvalidOperationException>(() => texture.SetPixels32([new Color32(1, 2, 3, 4)]), $"{texture.name} SetPixels32", "not readable");
            Throws<InvalidOperationException>(() => texture.LoadRawTextureData(new byte[64]), $"{texture.name} LoadRawTextureData", "not readable");
        }
        Ensure(ReferenceEquals(Texture2D.whiteTexture, Texture2D.whiteTexture), "a built-in texture is shared");
        Ensure(Texture2D.grayTexture.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && Texture2D.linearGrayTexture.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm, "linear gray is a linear format");
        Ensure(Texture2D.whiteTexture.name == "UnityWhite" && Texture2D.whiteTexture.hideFlags == HideFlags.HideAndDontSave, "built-in names and flags");
    }

    private static void CompressedFormats()
    {
        var dxt5 = new Texture2D(8, 8, TextureFormat.DXT5, false);
        Ensure(dxt5.GetRawTextureData().Length == 64 && dxt5.graphicsFormat == GraphicsFormat.RGBA_DXT5_SRGB, "DXT5 is 16 bytes per 4x4 block");
        var bytes = Enumerable.Range(0, 64).Select(i => (byte)(i * 3)).ToArray();
        dxt5.LoadRawTextureData(bytes);
        Ensure(dxt5.GetRawTextureData().SequenceEqual(bytes), "compressed data is copied in and out");
        Throws<NotSupportedException>(() => dxt5.GetPixel(0, 0), "compressed GetPixel", "GetPixel", "DXT5");
        Throws<NotSupportedException>(() => dxt5.SetPixel(0, 0, Color.red), "compressed SetPixel", "DXT5");
        Throws<NotSupportedException>(() => dxt5.GetPixels(), "compressed GetPixels", "DXT5");
        Throws<NotSupportedException>(() => dxt5.GetPixelBilinear(0.5f, 0.5f), "compressed GetPixelBilinear", "DXT5");
        dxt5.Apply();
        Ensure(dxt5.GetRawTextureData().SequenceEqual(bytes), "Apply leaves compressed data alone");
        var chain = new Texture2D(8, 8, TextureFormat.DXT1, true);
        Ensure(chain.mipmapCount == 4 && chain.GetRawTextureData().Length == 32 + 8 + 8 + 8, "compressed mip levels use whole blocks");
        Ensure(new Texture2D(5, 5, TextureFormat.ASTC_6x6, false).GetRawTextureData().Length == 16, "ASTC block sizes");
        Ensure(new Texture2D(4, 4, TextureFormat.PVRTC_RGB4, false).GetRawTextureData().Length == 32, "PVRTC has a minimum size");
        Throws<NotSupportedException>(() => new Texture2D(4, 4, TextureFormat.DXT5Crunched, false), "a crunched texture", "DXT5Crunched");
        var raw = new Texture2D(2, 2, TextureFormat.RGB9e5Float, false);
        Ensure(raw.GetRawTextureData().Length == 16, "an uncompressed format without a codec still allocates");
        Throws<NotSupportedException>(() => raw.GetPixel(0, 0), "a format without a codec", "RGB9e5Float");
    }

    private static void Readability()
    {
        var texture = new Texture2D(2, 2, TextureFormat.RGBA32, false);
        Ensure(texture.isReadable, "a script texture is readable");
        texture.Apply(false, makeNoLongerReadable: true);
        Ensure(!texture.isReadable, "Apply can drop readability");
        Throws<InvalidOperationException>(() => texture.GetPixel(0, 0), "reading a non-readable texture", "not readable");
        Throws<InvalidOperationException>(() => texture.GetRawTextureData(), "raw data of a non-readable texture", "not readable");
        Throws<InvalidOperationException>(() => texture.Apply(), "Apply on a non-readable texture", "not readable");
    }

    private static void FormatsAndConstructors()
    {
        Ensure(new Texture2D(2, 2, TextureFormat.RGBA32, false, false).graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, "linear false is sRGB");
        var linear = new Texture2D(2, 2, TextureFormat.RGBA32, false, true);
        Ensure(linear.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm && !linear.isDataSRGB, "linear true is UNorm");
        Ensure(new Texture2D(2, 2).format == TextureFormat.RGBA32 && new Texture2D(2, 2).mipmapCount == 2, "the default texture is RGBA32 with mips");
        Ensure(new Texture2D(2, 2, TextureFormat.RFloat, false).graphicsFormat == GraphicsFormat.R32_SFloat, "RFloat maps to R32_SFloat");
        var fromGraphics = new Texture2D(4, 4, GraphicsFormat.R16G16B16A16_SFloat, TextureCreationFlags.MipChain);
        Ensure(fromGraphics.format == TextureFormat.RGBAHalf && fromGraphics.mipmapCount == 3, "a GraphicsFormat constructor with a mip chain");
        Ensure(new Texture2D(4, 4, GraphicsFormat.R8G8B8A8_UNorm, TextureCreationFlags.None).mipmapCount == 1, "no mip chain flag");
        Ensure(new Texture2D(8, 8, GraphicsFormat.R8G8B8A8_SRGB, 2, TextureCreationFlags.None).mipmapCount == 2, "a GraphicsFormat mip count");
        Ensure(new Texture2D(4, 4, DefaultFormat.HDR, TextureCreationFlags.None).graphicsFormat == GraphicsFormat.R16G16B16A16_SFloat, "DefaultFormat.HDR");
        Ensure(new Texture2D(4, 4, DefaultFormat.LDR, 1, "group", TextureCreationFlags.IgnoreMipmapLimit) is { graphicsFormat: GraphicsFormat.R8G8B8A8_SRGB, mipmapLimitGroup: "group", ignoreMipmapLimit: true }, "DefaultFormat.LDR and mipmap limit group");
        Ensure(new Texture2D(4, 4, TextureFormat.RGBA32, 1, true, false, new MipmapLimitDescriptor(false, "g")) is { mipmapLimitGroup: "g", ignoreMipmapLimit: true }, "a mipmap limit descriptor");
        Throws<NotSupportedException>(() => new Texture2D(4, 4, GraphicsFormat.D32_SFloat, TextureCreationFlags.None), "a depth format", "D32_SFloat");
        Throws<NotSupportedException>(() => new Texture2D(4, 4, DefaultFormat.DepthStencil, TextureCreationFlags.None), "DefaultFormat.DepthStencil", "None");

        var resized = new Texture2D(2, 2, TextureFormat.RGBA32, false);
        resized.SetPixel(0, 0, Color.red);
        Ensure(resized.Reinitialize(8, 4) && resized.width == 8 && resized.height == 4 && resized.mipmapCount == 1, "Reinitialize changes the size");
        Ensure(resized.GetPixel(0, 0) == default && resized.GetRawTextureData().Length == 128, "Reinitialize clears the pixels");
        Ensure(resized.Reinitialize(4, 4, TextureFormat.RFloat, true) && resized.format == TextureFormat.RFloat && resized.mipmapCount == 3, "Reinitialize with a format and mips");
        Ensure(resized.Resize(2, 2) && resized.width == 2 && resized.Resize(2, 2, TextureFormat.RGBA32, false) && resized.format == TextureFormat.RGBA32, "Resize");
        Ensure(resized.Reinitialize(2, 2, GraphicsFormat.R8_UNorm, false) && resized.format == TextureFormat.R8 && resized.Resize(2, 2, GraphicsFormat.R8_UNorm, false), "Reinitialize with a GraphicsFormat");
        Throws<NotImplementedException>(() => resized.width = 4, "Texture2D.width cannot be set");
        Ensure(resized.dimension == UnityEngine.Rendering.TextureDimension.Tex2D, "a Texture2D is 2D");
        Throws<NotImplementedException>(() => resized.dimension = UnityEngine.Rendering.TextureDimension.Cube, "Texture2D.dimension cannot be set");
    }

    private static void SettingsAndStatics()
    {
        var texture = new Texture2D(4, 2, TextureFormat.RGBA32, false);
        Ensure(texture.filterMode == FilterMode.Bilinear && texture.anisoLevel == 1 && texture.mipMapBias == 0f && texture.wrapMode == TextureWrapMode.Repeat, "defaults");
        texture.filterMode = FilterMode.Trilinear;
        texture.anisoLevel = 8;
        texture.mipMapBias = -0.5f;
        Ensure(texture.filterMode == FilterMode.Trilinear && texture.anisoLevel == 8 && texture.mipMapBias == -0.5f, "sampler settings are stored");
        Ensure(texture.texelSize == new Vector2(0.25f, 0.5f) && texture.GetNativeTexturePtr() == IntPtr.Zero && texture.GetNativeTextureID() == 0, "texel size and native pointers");
        var before = texture.updateCount;
        texture.IncrementUpdateCount();
        Ensure(texture.updateCount == before + 1, "IncrementUpdateCount");
        Ensure(texture.requestedMipmapLevel == -1 && texture.minimumMipmapLevel == 0 && texture.IsRequestedMipmapLevelLoaded(), "streaming defaults");
        texture.requestedMipmapLevel = 2;
        texture.minimumMipmapLevel = 1;
        Ensure(texture.requestedMipmapLevel == 2 && texture.minimumMipmapLevel == 1 && texture.loadedMipmapLevel == 0, "requested and minimum levels are stored");
        texture.ClearRequestedMipmapLevel();
        texture.ClearMinimumMipmapLevel();
        Ensure(texture.requestedMipmapLevel == -1 && texture.minimumMipmapLevel == 0, "streaming levels clear");
        Throws<NotSupportedException>(() => _ = texture.graphicsTexture, "graphicsTexture", "graphicsTexture");

        var (limit, aniso, threaded, force, discard) = (Texture.globalMipmapLimit, Texture.anisotropicFiltering, Texture.allowThreadedTextureCreation,
            Texture.streamingTextureForceLoadAll, Texture.streamingTextureDiscardUnusedMips);
        Texture.globalMipmapLimit = 2;
        Texture.anisotropicFiltering = AnisotropicFiltering.ForceEnable;
        Texture.allowThreadedTextureCreation = true;
        Texture.streamingTextureForceLoadAll = true;
        Texture.streamingTextureDiscardUnusedMips = true;
        Ensure(Texture.globalMipmapLimit == 2 && Texture.anisotropicFiltering == AnisotropicFiltering.ForceEnable && Texture.allowThreadedTextureCreation
            && Texture.streamingTextureForceLoadAll && Texture.streamingTextureDiscardUnusedMips, "static settings are stored");
        (Texture.globalMipmapLimit, Texture.anisotropicFiltering, Texture.allowThreadedTextureCreation) = (limit, aniso, threaded);
        (Texture.streamingTextureForceLoadAll, Texture.streamingTextureDiscardUnusedMips) = (force, discard);
        Texture.SetGlobalAnisotropicFilteringLimits(2, 8);
        Texture.SetStreamingTextureMaterialDebugProperties();
        Ensure(Texture.currentTextureMemory == 0 && Texture.totalTextureMemory == 0 && Texture.streamingTextureCount == 0, "no texture is resident on a GPU");
    }

    private static void UnsupportedMembers()
    {
        var texture = new Texture2D(2, 2);
        NotSupported(() => texture.Compress(true), "Compress", "Texture2D.Compress");
        NotSupported(() => texture.ReadPixels(new Rect(0, 0, 2, 2), 0, 0), "ReadPixels", "Texture2D.ReadPixels");
        NotSupported(() => texture.ReadPixels(new Rect(0, 0, 2, 2), 0, 0, true), "ReadPixels with mips", "Texture2D.ReadPixels");
        NotSupported(() => texture.PackTextures([texture], 0), "PackTextures", "Texture2D.PackTextures");
        NotSupported(() => texture.PackTextures([texture], 0, 64), "PackTextures", "Texture2D.PackTextures");
        NotSupported(() => texture.PackTextures([texture], 0, 64, false), "PackTextures", "Texture2D.PackTextures");
        NotSupported(() => texture.CopyPixels(texture), "CopyPixels", "Texture2D.CopyPixels");
        NotSupported(() => texture.CopyPixels(texture, 0, 0, 0), "CopyPixels", "Texture2D.CopyPixels");
        NotSupported(() => texture.CopyPixels(texture, 0, 0, 0, 0, 1, 1, 0, 0, 0), "CopyPixels", "Texture2D.CopyPixels");
        NotSupported(() => Texture2D.GenerateAtlas([Vector2.one], 0, 64, []), "GenerateAtlas", "Texture2D.GenerateAtlas");
        NotSupported(() => Texture2D.CreateExternalTexture(2, 2, TextureFormat.RGBA32, false, false, IntPtr.Zero), "CreateExternalTexture", "Texture2D.CreateExternalTexture");
        NotSupported(() => texture.UpdateExternalTexture(IntPtr.Zero), "UpdateExternalTexture", "Texture2D.UpdateExternalTexture");
    }
}
