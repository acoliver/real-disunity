using System.Text.RegularExpressions;
using UnityEngine.Experimental.Rendering;

namespace UnityEngine;

/// <summary>
/// The channels a <see cref="GraphicsFormat"/> holds, read from its name: the part before the first underscore lists them,
/// as <c>R8G8B8A8</c> for an uncompressed format and <c>RGBA</c> for a compressed one. R, G and B are color channels and A
/// is alpha; the shared exponent of <c>E5B9G9R9</c> is not a channel. Depth and stencil formats hold neither.
/// </summary>
internal static partial class GraphicsFormatComponents
{
    [GeneratedRegex("[RGBA]")]
    private static partial Regex Channel();

    [GeneratedRegex("^(?:[RGBA]16)+_SFloat$")]
    private static partial Regex HalfFloat();

    [GeneratedRegex("_(UNorm|SNorm|UInt|SInt|SFloat|UFloat|SRGB)(?:Pack\\d+)?$")]
    private static partial Regex NumericSuffix();

    /// <summary>How a color format stores each channel, read from the end of its name; sRGB channels are unsigned normalized.
    /// A depth, stencil or empty format has no kind.</summary>
    internal enum Numeric
    {
        None, UNorm, SNorm, UInt, SInt, SFloat, UFloat,
    }

    internal static Numeric Kind(GraphicsFormat format)
    {
        if (format == GraphicsFormat.None || GraphicsFormats.DepthBits(format) > 0 || GraphicsFormats.HasStencil(format)) return Numeric.None;
        return NumericSuffix().Match(format.ToString()).Groups[1].Value switch
        {
            "UNorm" or "SRGB" => Numeric.UNorm,
            "SNorm" => Numeric.SNorm,
            "UInt" => Numeric.UInt,
            "SInt" => Numeric.SInt,
            "SFloat" => Numeric.SFloat,
            "UFloat" => Numeric.UFloat,
            _ => Numeric.None,
        };
    }

    internal static bool IsHalf(GraphicsFormat format) => HalfFloat().IsMatch(format.ToString());

    internal static bool IsIEEE754(GraphicsFormat format) => Kind(format) == Numeric.SFloat;

    internal static uint Color(GraphicsFormat format) => (uint)Letters(format).Count(letter => letter != 'A');

    internal static uint Alpha(GraphicsFormat format) => (uint)Letters(format).Count(letter => letter == 'A');

    private static IEnumerable<char> Letters(GraphicsFormat format)
    {
        if (format == GraphicsFormat.None || GraphicsFormats.DepthBits(format) > 0 || GraphicsFormats.HasStencil(format)) return [];
        var name = format.ToString();
        var end = name.IndexOf('_', StringComparison.Ordinal);
        return Channel().Matches(end < 0 ? name : name[..end]).Select(match => match.Value[0]);
    }
}
