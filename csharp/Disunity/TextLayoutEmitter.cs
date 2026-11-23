namespace UnityEngine;

/// <summary>The placed text an emitter turns into vertices: the characters, the placed lines and how many of them are kept.</summary>
internal readonly record struct EmitInput(IReadOnlyList<LayoutChar> Chars, IReadOnlyList<PlacedLine> Lines, int Kept);

/// <summary>Turns placed lines into the lists UI Text reads: a quad of four vertices for each glyph with ink on a kept
/// line, a character for every laid-out character and a line for every kept line. A quad runs top left, top right, bottom
/// right, bottom left, so its two triangles (0, 1, 2) and (2, 3, 0) stay an upright rectangle with its UVs.</summary>
internal static class TextLayoutEmitter
{
    public static void Emit(TextLayoutResult result, EmitInput input, IReadOnlyDictionary<GlyphKey, CharacterInfo> infos)
    {
        for (var k = 0; k < input.Lines.Count; k++)
        {
            var placed = input.Lines[k];
            var nextStart = k + 1 < input.Lines.Count ? input.Lines[k + 1].Line.Start : input.Chars.Count;
            var pen = EmitLine(result, input, (placed, k < input.Kept), infos);
            if (placed.Line.End < nextStart) AddCharacter(result, pen, placed.Top, input.Chars[placed.Line.End].Advance);
            if (k >= input.Kept) continue;
            var measures = placed.Measures;
            result.Lines.Add(new UILineInfo { startCharIdx = placed.Line.Start, height = measures.Height, topY = placed.Top, leading = measures.Step - measures.Height });
        }
    }

    private static float EmitLine(TextLayoutResult result, EmitInput input, (PlacedLine Placed, bool Keep) line, IReadOnlyDictionary<GlyphKey, CharacterInfo> infos)
    {
        var (placed, keep) = line;
        var pen = placed.X;
        for (var i = placed.Line.Start; i < placed.Line.End; i++)
        {
            var c = input.Chars[i];
            if (i > placed.Line.Start) pen += c.Kern;
            AddCharacter(result, pen, placed.Top, c.Advance);
            var key = new GlyphKey(c.Glyph, c.Style.Size, c.Style.Style);
            if (keep && c.Glyph != 0 && infos.TryGetValue(key, out var info) && info.glyphWidth > 0)
                AddQuad(result.Verts, pen, placed.Baseline, info, c.Style.Color);
            pen += c.Advance;
        }
        return pen;
    }

    private static void AddCharacter(TextLayoutResult result, float x, float top, int advance) =>
        result.Characters.Add(new UICharInfo { cursorPos = new Vector2(x, top), charWidth = advance });

    private static void AddQuad(List<UIVertex> verts, float pen, float baseline, CharacterInfo info, Color32 color)
    {
        float left = pen + info.minX, right = pen + info.maxX, top = baseline + info.maxY, bottom = baseline + info.minY;
        verts.Add(Vertex(left, top, info.uvTopLeft, color));
        verts.Add(Vertex(right, top, info.uvTopRight, color));
        verts.Add(Vertex(right, bottom, info.uvBottomRight, color));
        verts.Add(Vertex(left, bottom, info.uvBottomLeft, color));
    }

    private static UIVertex Vertex(float x, float y, Vector2 uv, Color32 color)
    {
        var vertex = UIVertex.simpleVert;
        vertex.position = new Vector3(x, y, 0f);
        vertex.uv0 = new Vector4(uv.x, uv.y, 0f, 0f);
        vertex.color = color;
        return vertex;
    }

    /// <summary>The rectangle the kept lines cover: from the leftmost line start to the rightmost line end, and from the
    /// last line's bottom to the first line's top. Empty with no kept line.</summary>
    public static Rect Extents(IEnumerable<PlacedLine> kept)
    {
        var lines = kept.ToList();
        if (lines.Count == 0) return default;
        var left = lines.Min(l => l.X);
        var right = lines.Max(l => l.X + l.Line.Width);
        var bottom = lines[^1].Top - lines[^1].Measures.Height;
        return new Rect(left, bottom, right - left, lines[0].Top - bottom);
    }
}
