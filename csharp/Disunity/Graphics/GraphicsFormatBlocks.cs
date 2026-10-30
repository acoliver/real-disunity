using System.Text.RegularExpressions;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine;

/// <summary>
/// The size in bytes and the texel footprint of one block of a <see cref="GraphicsFormat"/>. An uncompressed texel is a
/// 1x1 block. The values come from the format's name: the bit width of each channel, the width of a packed
/// format, and the codec of a compressed format.
/// </summary>
internal static partial class GraphicsFormatBlocks
{
    [GeneratedRegex(@"Pack(\d+)")]
    private static partial Regex PackWidth();

    [GeneratedRegex(@"[A-Z](\d+)")]
    private static partial Regex ChannelBits();

    [GeneratedRegex(@"ASTC(\d+)X(\d+)")]
    private static partial Regex AstcFootprint();

    internal static uint Bytes(GraphicsFormat format)
    {
        var name = format.ToString();
        if (format == GraphicsFormat.None) return 0;
        if (GraphicsFormats.DepthBits(format) > 0 || GraphicsFormats.HasStencil(format)) return DepthBytes(format);
        if (IsCompressed(name)) return CompressedBytes(name);
        var pack = PackWidth().Match(name);
        if (pack.Success) return uint.Parse(pack.Groups[1].Value) / 8;
        var channels = name[..name.IndexOf('_', StringComparison.Ordinal)];
        return (uint)ChannelBits().Matches(channels).Sum(m => int.Parse(m.Groups[1].Value)) / 8;
    }

    internal static uint Width(GraphicsFormat format) => Footprint(format.ToString(), first: true);

    internal static uint Height(GraphicsFormat format) => Footprint(format.ToString(), first: false);

    private static bool IsCompressed(string name) =>
        name.Contains("DXT") || name.Contains("BC") || name.Contains("ETC") || name.Contains("EAC") || name.Contains("ASTC") || name.Contains("PVRTC");

    private static uint DepthBytes(GraphicsFormat format) => format switch
    {
        GraphicsFormat.S8_UInt => 1,
        GraphicsFormat.D16_UNorm => 2,
        GraphicsFormat.D16_UNorm_S8_UInt => 3,
        GraphicsFormat.D24_UNorm or GraphicsFormat.D24_UNorm_S8_UInt or GraphicsFormat.D32_SFloat => 4,
        GraphicsFormat.D32_SFloat_S8_UInt => 5,
        _ => throw new NotSupportedException($"GraphicsFormat.{format} has no block size yet"),
    };

    private static uint CompressedBytes(string name) =>
        name.Contains("ASTC") || name.Contains("DXT3") || name.Contains("DXT5") || name.Contains("BC2") || name.Contains("BC3")
        || name.Contains("BC5") || name.Contains("BC6H") || name.Contains("BC7") || name.StartsWith("RGBA_ETC2", StringComparison.Ordinal)
        || name.StartsWith("RG_EAC", StringComparison.Ordinal) ? 16u : 8u;

    private static uint Footprint(string name, bool first)
    {
        var astc = AstcFootprint().Match(name);
        if (astc.Success) return uint.Parse(astc.Groups[first ? 1 : 2].Value);
        if (name.Contains("PVRTC_2Bpp")) return first ? 8u : 4u;
        return IsCompressed(name) ? 4u : 1u;
    }
}
