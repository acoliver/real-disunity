using System.Buffers.Binary;

namespace UnityEngine;

/// <summary>
/// Byte sizes of texture mip levels and the pixel codecs of the uncompressed formats the engine
/// reads and writes. Every other format keeps raw bytes only.
/// </summary>
internal static partial class TexelFormats
{
    private readonly record struct Block(int Width, int Height, int Bytes, int MinBlocks);

    private static readonly Dictionary<TextureFormat, int> PixelBytes = new()
    {
        [TextureFormat.Alpha8] = 1, [TextureFormat.R8] = 1, [TextureFormat.R8_SIGNED] = 1,
        [TextureFormat.ARGB4444] = 2, [TextureFormat.RGBA4444] = 2, [TextureFormat.RGB565] = 2,
        [TextureFormat.R16] = 2, [TextureFormat.RHalf] = 2, [TextureFormat.RG16] = 2,
        [TextureFormat.RG16_SIGNED] = 2, [TextureFormat.R16_SIGNED] = 2, [TextureFormat.YUY2] = 2,
        [TextureFormat.RGB24] = 3, [TextureFormat.RGB24_SIGNED] = 3,
        [TextureFormat.RGBA32] = 4, [TextureFormat.ARGB32] = 4, [TextureFormat.BGRA32] = 4,
        [TextureFormat.RGHalf] = 4, [TextureFormat.RFloat] = 4, [TextureFormat.RGB9e5Float] = 4,
        [TextureFormat.RG32] = 4, [TextureFormat.RGBA32_SIGNED] = 4, [TextureFormat.RG32_SIGNED] = 4,
        [TextureFormat.RGB48] = 6, [TextureFormat.RGB48_SIGNED] = 6,
        [TextureFormat.RGBAHalf] = 8, [TextureFormat.RGFloat] = 8, [TextureFormat.RGBA64] = 8, [TextureFormat.RGBA64_SIGNED] = 8,
        [TextureFormat.RGBAFloat] = 16,
    };

    private static readonly Dictionary<TextureFormat, Block> Blocks = BuildBlocks();

    private static readonly HashSet<TextureFormat> Crunched =
    [
        TextureFormat.DXT1Crunched, TextureFormat.DXT5Crunched, TextureFormat.ETC_RGB4Crunched, TextureFormat.ETC2_RGBA8Crunched,
    ];

    private static Dictionary<TextureFormat, Block> BuildBlocks()
    {
        var table = new Dictionary<TextureFormat, Block>();
        foreach (var format in new[] { TextureFormat.DXT1, TextureFormat.BC4, TextureFormat.ETC_RGB4, TextureFormat.ETC2_RGB,
            TextureFormat.ETC2_RGBA1, TextureFormat.EAC_R, TextureFormat.EAC_R_SIGNED })
            table[format] = new Block(4, 4, 8, 1);
        foreach (var format in new[] { TextureFormat.DXT5, TextureFormat.BC5, TextureFormat.BC6H, TextureFormat.BC7,
            TextureFormat.ETC2_RGBA8, TextureFormat.EAC_RG, TextureFormat.EAC_RG_SIGNED })
            table[format] = new Block(4, 4, 16, 1);
        foreach (var (format, size) in new[] { (TextureFormat.ASTC_4x4, 4), (TextureFormat.ASTC_5x5, 5), (TextureFormat.ASTC_6x6, 6),
            (TextureFormat.ASTC_8x8, 8), (TextureFormat.ASTC_10x10, 10), (TextureFormat.ASTC_12x12, 12),
            (TextureFormat.ASTC_HDR_4x4, 4), (TextureFormat.ASTC_HDR_5x5, 5), (TextureFormat.ASTC_HDR_6x6, 6),
            (TextureFormat.ASTC_HDR_8x8, 8), (TextureFormat.ASTC_HDR_10x10, 10), (TextureFormat.ASTC_HDR_12x12, 12) })
            table[format] = new Block(size, size, 16, 1);
        table[TextureFormat.PVRTC_RGB4] = table[TextureFormat.PVRTC_RGBA4] = new Block(4, 4, 8, 2);
        table[TextureFormat.PVRTC_RGB2] = table[TextureFormat.PVRTC_RGBA2] = new Block(8, 4, 8, 2);
        return table;
    }

    /// <summary>Whether the engine can allocate a texture of the format; crunched formats have no fixed size.</summary>
    internal static bool CanAllocate(TextureFormat format) => PixelBytes.ContainsKey(format) || Blocks.ContainsKey(format);

    internal static bool IsCrunched(TextureFormat format) => Crunched.Contains(format);

    internal static bool IsCompressed(TextureFormat format) => Blocks.ContainsKey(format) || Crunched.Contains(format);

    /// <summary>The bytes of one pixel of an uncompressed format.</summary>
    internal static int BytesPerPixel(TextureFormat format) =>
        PixelBytes.TryGetValue(format, out var bytes) ? bytes : throw new NotSupportedException($"TextureFormat.{format} is not an uncompressed format");

    /// <summary>The bytes of a mip level of the given size.</summary>
    internal static long LevelSize(TextureFormat format, int width, int height)
    {
        if (PixelBytes.TryGetValue(format, out var bytes)) return (long)width * height * bytes;
        if (!Blocks.TryGetValue(format, out var block)) throw new NotSupportedException($"TextureFormat.{format} has no fixed size");
        var across = Math.Max((width + block.Width - 1) / block.Width, block.MinBlocks);
        var down = Math.Max((height + block.Height - 1) / block.Height, block.MinBlocks);
        return (long)across * down * block.Bytes;
    }

    /// <summary>The number of mip levels of a full chain, down to one pixel.</summary>
    internal static int FullChain(int width, int height) => 1 + (int)Math.Floor(Math.Log2(Math.Max(width, height)));

    /// <summary>Whether <see cref="Read"/> and <see cref="Write"/> handle the format.</summary>
    internal static bool HasCodec(TextureFormat format) => format switch
    {
        TextureFormat.Alpha8 or TextureFormat.R8 or TextureFormat.RG16 or TextureFormat.RGB24 or TextureFormat.RGBA32
            or TextureFormat.ARGB32 or TextureFormat.BGRA32 or TextureFormat.R16 or TextureFormat.RG32 or TextureFormat.RGB48
            or TextureFormat.RGBA64 or TextureFormat.RHalf or TextureFormat.RGHalf or TextureFormat.RGBAHalf
            or TextureFormat.RFloat or TextureFormat.RGFloat or TextureFormat.RGBAFloat or TextureFormat.RGB565
            or TextureFormat.RGBA4444 or TextureFormat.ARGB4444 => true,
        _ => false,
    };

    private static float Unorm8(byte value) => value / 255f;
    private static float Unorm16(ReadOnlySpan<byte> pixel, int index) => BinaryPrimitives.ReadUInt16LittleEndian(pixel[(index * 2)..]) / 65535f;
    private static float Half(ReadOnlySpan<byte> pixel, int index) => (float)BinaryPrimitives.ReadHalfLittleEndian(pixel[(index * 2)..]);
    private static float Single(ReadOnlySpan<byte> pixel, int index) => BinaryPrimitives.ReadSingleLittleEndian(pixel[(index * 4)..]);
    private static byte ToUnorm8(float value) => (byte)Math.Clamp((int)MathF.Round(value * 255f), 0, 255);
    private static ushort ToUnorm16(float value) => (ushort)Math.Clamp((int)MathF.Round(value * 65535f), 0, 65535);
    private static int Bits(float value, int max) => Math.Clamp((int)MathF.Round(value * max), 0, max);

    /// <summary>The color of one pixel. Channels a format lacks read as 0 and alpha as 1.</summary>
    internal static Color Read(TextureFormat format, ReadOnlySpan<byte> p) => format switch
    {
        TextureFormat.Alpha8 => new(1, 1, 1, Unorm8(p[0])),
        TextureFormat.R8 => new(Unorm8(p[0]), 0, 0, 1),
        TextureFormat.RG16 => new(Unorm8(p[0]), Unorm8(p[1]), 0, 1),
        TextureFormat.RGB24 => new(Unorm8(p[0]), Unorm8(p[1]), Unorm8(p[2]), 1),
        TextureFormat.RGBA32 => new(Unorm8(p[0]), Unorm8(p[1]), Unorm8(p[2]), Unorm8(p[3])),
        TextureFormat.ARGB32 => new(Unorm8(p[1]), Unorm8(p[2]), Unorm8(p[3]), Unorm8(p[0])),
        TextureFormat.BGRA32 => new(Unorm8(p[2]), Unorm8(p[1]), Unorm8(p[0]), Unorm8(p[3])),
        TextureFormat.R16 => new(Unorm16(p, 0), 0, 0, 1),
        TextureFormat.RG32 => new(Unorm16(p, 0), Unorm16(p, 1), 0, 1),
        TextureFormat.RGB48 => new(Unorm16(p, 0), Unorm16(p, 1), Unorm16(p, 2), 1),
        TextureFormat.RGBA64 => new(Unorm16(p, 0), Unorm16(p, 1), Unorm16(p, 2), Unorm16(p, 3)),
        TextureFormat.RHalf => new(Half(p, 0), 0, 0, 1),
        TextureFormat.RGHalf => new(Half(p, 0), Half(p, 1), 0, 1),
        TextureFormat.RGBAHalf => new(Half(p, 0), Half(p, 1), Half(p, 2), Half(p, 3)),
        TextureFormat.RFloat => new(Single(p, 0), 0, 0, 1),
        TextureFormat.RGFloat => new(Single(p, 0), Single(p, 1), 0, 1),
        TextureFormat.RGBAFloat => new(Single(p, 0), Single(p, 1), Single(p, 2), Single(p, 3)),
        _ => ReadPacked(format, BinaryPrimitives.ReadUInt16LittleEndian(p)),
    };

    private static Color ReadPacked(TextureFormat format, ushort v) => format switch
    {
        TextureFormat.RGB565 => new(((v >> 11) & 31) / 31f, ((v >> 5) & 63) / 63f, (v & 31) / 31f, 1),
        TextureFormat.RGBA4444 => new(((v >> 12) & 15) / 15f, ((v >> 8) & 15) / 15f, ((v >> 4) & 15) / 15f, (v & 15) / 15f),
        TextureFormat.ARGB4444 => new(((v >> 8) & 15) / 15f, ((v >> 4) & 15) / 15f, (v & 15) / 15f, ((v >> 12) & 15) / 15f),
        _ => throw new NotSupportedException($"TextureFormat.{format} pixels cannot be read yet"),
    };

    /// <summary>Stores a color in one pixel, keeping the channels the format has.</summary>
    internal static void Write(TextureFormat format, Span<byte> p, Color c)
    {
        switch (format)
        {
            case TextureFormat.Alpha8: p[0] = ToUnorm8(c.a); break;
            case TextureFormat.R8: p[0] = ToUnorm8(c.r); break;
            case TextureFormat.RG16: p[0] = ToUnorm8(c.r); p[1] = ToUnorm8(c.g); break;
            case TextureFormat.RGB24: p[0] = ToUnorm8(c.r); p[1] = ToUnorm8(c.g); p[2] = ToUnorm8(c.b); break;
            case TextureFormat.RGBA32: p[0] = ToUnorm8(c.r); p[1] = ToUnorm8(c.g); p[2] = ToUnorm8(c.b); p[3] = ToUnorm8(c.a); break;
            case TextureFormat.ARGB32: p[0] = ToUnorm8(c.a); p[1] = ToUnorm8(c.r); p[2] = ToUnorm8(c.g); p[3] = ToUnorm8(c.b); break;
            case TextureFormat.BGRA32: p[0] = ToUnorm8(c.b); p[1] = ToUnorm8(c.g); p[2] = ToUnorm8(c.r); p[3] = ToUnorm8(c.a); break;
            default: WriteWide(format, p, c); break;
        }
    }

    private static void WriteWide(TextureFormat format, Span<byte> p, Color c)
    {
        switch (format)
        {
            case TextureFormat.R16: Put16(p, 0, ToUnorm16(c.r)); break;
            case TextureFormat.RG32: Put16(p, 0, ToUnorm16(c.r)); Put16(p, 1, ToUnorm16(c.g)); break;
            case TextureFormat.RGB48: PutUnorm16(p, c, 3); break;
            case TextureFormat.RGBA64: PutUnorm16(p, c, 4); break;
            case TextureFormat.RHalf: PutHalf(p, c, 1); break;
            case TextureFormat.RGHalf: PutHalf(p, c, 2); break;
            case TextureFormat.RGBAHalf: PutHalf(p, c, 4); break;
            case TextureFormat.RFloat: PutSingle(p, c, 1); break;
            case TextureFormat.RGFloat: PutSingle(p, c, 2); break;
            case TextureFormat.RGBAFloat: PutSingle(p, c, 4); break;
            default: BinaryPrimitives.WriteUInt16LittleEndian(p, Pack(format, c)); break;
        }
    }

    private static void Put16(Span<byte> p, int index, ushort value) => BinaryPrimitives.WriteUInt16LittleEndian(p[(index * 2)..], value);

    private static void PutUnorm16(Span<byte> p, Color c, int channels)
    {
        for (var i = 0; i < channels; i++) Put16(p, i, ToUnorm16(c[i]));
    }

    private static void PutHalf(Span<byte> p, Color c, int channels)
    {
        for (var i = 0; i < channels; i++) BinaryPrimitives.WriteHalfLittleEndian(p[(i * 2)..], (Half)c[i]);
    }

    private static void PutSingle(Span<byte> p, Color c, int channels)
    {
        for (var i = 0; i < channels; i++) BinaryPrimitives.WriteSingleLittleEndian(p[(i * 4)..], c[i]);
    }

    private static ushort Pack(TextureFormat format, Color c) => format switch
    {
        TextureFormat.RGB565 => (ushort)((Bits(c.r, 31) << 11) | (Bits(c.g, 63) << 5) | Bits(c.b, 31)),
        TextureFormat.RGBA4444 => (ushort)((Bits(c.r, 15) << 12) | (Bits(c.g, 15) << 8) | (Bits(c.b, 15) << 4) | Bits(c.a, 15)),
        TextureFormat.ARGB4444 => (ushort)((Bits(c.a, 15) << 12) | (Bits(c.r, 15) << 8) | (Bits(c.g, 15) << 4) | Bits(c.b, 15)),
        _ => throw new NotSupportedException($"TextureFormat.{format} pixels cannot be written yet"),
    };
}
