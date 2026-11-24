namespace UnityEngine;

/// <summary>Where the ink of a line sits: how far from the line's start its first inked glyph begins, and how wide the ink
/// is from that point to the last inked glyph's end. A line with no ink has none.</summary>
internal readonly record struct InkBox(float Offset, float Width);

/// <summary>Finds the ink of lines from the face's metrics, with the same rounding that draws the glyphs: a glyph's ink
/// starts at its rounded bearing and is as wide as its rectangle in the atlas, the ceiling of its width.</summary>
internal static class TextLayoutInk
{
    /// <summary>The box a line is aligned by: its ink when aligning by geometry and the line has ink, otherwise the whole line.</summary>
    public static InkBox Of(IReadOnlyList<LayoutChar> chars, TextLine line, FontGlyphs glyphs, bool byGeometry)
    {
        var whole = new InkBox(0, line.Width);
        if (!byGeometry) return whole;
        var (first, last) = (float.NaN, float.NaN);
        var pen = 0f;
        for (var i = line.Start; i < line.End; i++)
        {
            if (i > line.Start) pen += chars[i].Kern;
            if (TryInk(chars[i], glyphs, out var left, out var width))
            {
                if (float.IsNaN(first)) first = pen + left;
                last = pen + left + width;
            }
            pen += chars[i].Advance;
        }
        return float.IsNaN(first) ? whole : new InkBox(first, last - first);
    }

    private static bool TryInk(LayoutChar c, FontGlyphs glyphs, out float left, out float width)
    {
        (left, width) = (0, 0);
        if (c.Glyph == 0 || !glyphs.TryMetrics(c.Style.Size, c.Glyph, out var metrics) || metrics.width <= 0 || metrics.height <= 0) return false;
        (left, width) = (MathF.Round(metrics.horizontalBearingX), MathF.Ceiling(metrics.width));
        return true;
    }
}
