namespace UnityEngine.TextCore.LowLevel;

/// <summary>
/// Places rectangles in the free space of an atlas by the maximal rectangles method: the free list holds
/// every largest free rectangle, a placement is cut out of each free rectangle it overlaps, and a free rectangle
/// inside another is dropped. Rectangles are never rotated. The packing mode picks among the free rectangles
/// that fit; ties go to the first in the list.
/// </summary>
internal static class FontAtlasPacker
{
    /// <summary>Finds room for a <paramref name="width"/> by <paramref name="height"/> rectangle, records it in
    /// <paramref name="used"/> and cuts it out of <paramref name="free"/>. Nothing changes when none fits.</summary>
    public static bool TryPlace(GlyphPackingMode mode, int width, int height, int atlasWidth, int atlasHeight,
        List<GlyphRect> free, List<GlyphRect> used, out GlyphRect placed)
    {
        placed = default;
        var best = -1;
        var bestScore = (long.MaxValue, long.MaxValue);
        for (var i = 0; i < free.Count; i++)
        {
            var slot = free[i];
            if (slot.width < width || slot.height < height) continue;
            var candidate = new GlyphRect(slot.x, slot.y, width, height);
            var score = Score(mode, slot, candidate, atlasWidth, atlasHeight, used);
            if (score.CompareTo(bestScore) >= 0) continue;
            (best, bestScore, placed) = (i, score, candidate);
        }
        if (best < 0) return false;
        Cut(free, placed);
        used.Add(placed);
        return true;
    }

    /// <summary>Lower is better.</summary>
    private static (long Primary, long Secondary) Score(GlyphPackingMode mode, GlyphRect slot, GlyphRect candidate,
        int atlasWidth, int atlasHeight, List<GlyphRect> used)
    {
        long leftoverWidth = slot.width - candidate.width, leftoverHeight = slot.height - candidate.height;
        var shortSide = Math.Min(leftoverWidth, leftoverHeight);
        var longSide = Math.Max(leftoverWidth, leftoverHeight);
        return mode switch
        {
            GlyphPackingMode.BestShortSideFit => (shortSide, longSide),
            GlyphPackingMode.BestLongSideFit => (longSide, shortSide),
            GlyphPackingMode.BestAreaFit => ((long)slot.width * slot.height, shortSide),
            GlyphPackingMode.BottomLeftRule => ((long)candidate.y + candidate.height, candidate.x),
            GlyphPackingMode.ContactPointRule => (-Contact(candidate, atlasWidth, atlasHeight, used), 0),
            _ => throw new ArgumentOutOfRangeException(nameof(mode), mode, "unknown packing mode"),
        };
    }

    /// <summary>The length of the rectangle's edge that touches the atlas border or a used rectangle.</summary>
    private static long Contact(GlyphRect r, int atlasWidth, int atlasHeight, List<GlyphRect> used)
    {
        long score = 0;
        if (r.x == 0 || r.x + r.width == atlasWidth) score += r.height;
        if (r.y == 0 || r.y + r.height == atlasHeight) score += r.width;
        foreach (var other in used)
        {
            if (other.x == r.x + r.width || other.x + other.width == r.x) score += Overlap(r.y, r.y + r.height, other.y, other.y + other.height);
            if (other.y == r.y + r.height || other.y + other.height == r.y) score += Overlap(r.x, r.x + r.width, other.x, other.x + other.width);
        }
        return score;
    }

    private static int Overlap(int start, int end, int otherStart, int otherEnd) => Math.Max(0, Math.Min(end, otherEnd) - Math.Max(start, otherStart));

    private static bool Intersects(GlyphRect a, GlyphRect b) =>
        a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;

    private static bool Contains(GlyphRect outer, GlyphRect inner) =>
        inner.x >= outer.x && inner.y >= outer.y && inner.x + inner.width <= outer.x + outer.width && inner.y + inner.height <= outer.y + outer.height;

    /// <summary>Removes the placed rectangle's area from every free rectangle it overlaps, then drops the free
    /// rectangles that lie inside another.</summary>
    private static void Cut(List<GlyphRect> free, GlyphRect placed)
    {
        for (var i = free.Count - 1; i >= 0; i--)
        {
            var slot = free[i];
            if (!Intersects(slot, placed)) continue;
            free.RemoveAt(i);
            if (placed.x > slot.x) free.Add(new GlyphRect(slot.x, slot.y, placed.x - slot.x, slot.height));
            if (placed.x + placed.width < slot.x + slot.width)
                free.Add(new GlyphRect(placed.x + placed.width, slot.y, slot.x + slot.width - placed.x - placed.width, slot.height));
            if (placed.y > slot.y) free.Add(new GlyphRect(slot.x, slot.y, slot.width, placed.y - slot.y));
            if (placed.y + placed.height < slot.y + slot.height)
                free.Add(new GlyphRect(slot.x, placed.y + placed.height, slot.width, slot.y + slot.height - placed.y - placed.height));
        }
        Prune(free);
    }

    private static void Prune(List<GlyphRect> free)
    {
        for (var i = free.Count - 1; i >= 0; i--)
            for (var j = 0; j < free.Count; j++)
            {
                if (i == j || !Contains(free[j], free[i]) || (free[i] == free[j] && i < j)) continue;
                free.RemoveAt(i);
                break;
            }
    }
}
