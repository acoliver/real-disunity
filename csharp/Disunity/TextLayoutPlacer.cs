namespace UnityEngine;

/// <summary>A line placed in the text rectangle: its start x, its top and its baseline, with y growing upward.</summary>
internal readonly record struct PlacedLine(TextLine Line, float X, float Top, float Baseline);

/// <summary>The measures of one line height: the distance from the top of a line down to its baseline, the line's height,
/// and the step from one line's top to the next, which is the height scaled by the line spacing.</summary>
internal readonly record struct LineMeasures(int Ascent, int Height, int Step);

/// <summary>Where the text rectangle is and how the text sits in it: the rectangle's left edge and top, its size, the
/// horizontal and vertical anchor factors (0 for left or upper, 0.5 for centred, 1 for right or lower) and whether lines
/// that pass the bottom are dropped.</summary>
internal readonly record struct TextArea(float Left, float Top, Vector2 Size, Vector2 Anchor, bool Truncate);

/// <summary>Places lines in a text rectangle by the rule of the engine's native text: the block is as tall as its lines'
/// steps plus the last line's height, a line passing the bottom is dropped when truncating, and the block and each line
/// sit in the space that is left by the anchor factors.</summary>
internal static class TextLayoutPlacer
{
    /// <summary>The factors of an anchor: x is 0, 0.5 or 1 from left to right, y is 0, 0.5 or 1 from upper to lower.</summary>
    public static Vector2 AnchorFactors(TextAnchor anchor) => new((int)anchor % 3 * 0.5f, (int)anchor / 3 * 0.5f);

    /// <summary>All the lines placed, and how many of them are kept: the lines from the first on that stay inside the
    /// rectangle's bottom when truncating, otherwise every line. A dropped line is still placed where it would have been.</summary>
    public static (List<PlacedLine> Lines, int Kept) Place(IReadOnlyList<TextLine> lines, LineMeasures measures, TextArea area)
    {
        var kept = lines.Count;
        if (area.Truncate)
            for (var i = 0; i < lines.Count; i++)
                if (i * measures.Step + measures.Height > area.Size.y) { kept = i; break; }
        var height = kept == 0 ? 0 : (kept - 1) * measures.Step + measures.Height;
        var blockTop = area.Top - (area.Size.y - height) * area.Anchor.y;
        var placed = new List<PlacedLine>(lines.Count);
        for (var i = 0; i < lines.Count; i++)
        {
            var top = blockTop - i * measures.Step;
            var x = area.Left + (area.Size.x - lines[i].Width) * area.Anchor.x;
            placed.Add(new PlacedLine(lines[i], x, top, top - measures.Ascent));
        }
        return (placed, kept);
    }
}
