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
