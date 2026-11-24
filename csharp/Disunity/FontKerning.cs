namespace UnityEngine;

/// <summary>
/// The kerning of one font's face, asked for in the pairs a text needs and kept per pixel size. A pair is asked once: the
/// face answers for every ordered pair of the glyphs of one request, and an answer of nothing is kept as well, so laying
/// out the same text again makes no native call. The adjustment is whole pixels like the advances it adds to.
/// </summary>
internal sealed class FontKerning(FontGlyphs glyphs)
{
    private readonly Dictionary<(int Size, uint Left, uint Right), int> _adjustments = new();
    private readonly HashSet<(int Size, uint Left, uint Right)> _asked = [];

    public void Clear()
    {
        _adjustments.Clear();
        _asked.Clear();
    }

    /// <summary>Asks the face for the pairs of this size that have not been asked for yet.</summary>
    public void Prepare(int size, IEnumerable<(uint Left, uint Right)> pairs)
    {
        var missing = pairs.Where(pair => !_asked.Contains((size, pair.Left, pair.Right))).Distinct().ToList();
        if (missing.Count == 0 || !glyphs.TryUseSize(size, out var face)) return;
        foreach (var record in face.Pairs(missing.SelectMany(pair => new[] { pair.Left, pair.Right }).Distinct().ToArray()))
            _adjustments[(size, record.first, record.second)] = (int)MathF.Round(record.x_advance);
        foreach (var pair in missing) _asked.Add((size, pair.Left, pair.Right));
    }

    /// <summary>The pixels added to the advance of the left glyph when the right glyph follows it; 0 for a pair that was not prepared.</summary>
    public int Adjustment(int size, uint left, uint right) => _adjustments.GetValueOrDefault((size, left, right));
}
