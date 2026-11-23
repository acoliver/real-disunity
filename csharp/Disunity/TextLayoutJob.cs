
namespace UnityEngine;

/// <summary>What a layout produced: the quads, the characters, the lines and the rectangle the kept lines cover, with the
/// number of characters on kept lines and the font size the text was laid out at.</summary>
internal sealed class TextLayoutResult(int visible, int fontSize)
{
    public List<UIVertex> Verts { get; } = [];
    public List<UICharInfo> Characters { get; } = [];
    public List<UILineInfo> Lines { get; } = [];
    public Rect Extents { get; set; }
    public int Visible { get; } = visible;
    public int FontSize { get; } = fontSize;
}

/// <summary>The characters of a layout, the lines they break into and the measures of each line.</summary>
internal sealed record Arranged(List<LayoutChar> Chars, List<TextLine> Lines, List<LineMeasures> Measures)
{
    /// <summary>The height of all the lines: the steps of all but the last line, and the last line's height.</summary>
    public float Height => Measures.Count == 0 ? 0 : Measures.Take(Measures.Count - 1).Sum(m => m.Step) + Measures[^1].Height;
}

/// <summary>
/// One layout of a string with one generation settings value: it reads the rich text tags, measures the characters, breaks
/// them into lines, places the lines and turns them into vertices. All lengths are in pixels at the settings' scale factor,
/// measured from the pivot with y growing upward. Characters and line breaks need only the face's metrics; the atlas is
/// touched only by <see cref="Layout"/>, so the preferred sizes never add glyphs.
/// </summary>
internal sealed class TextLayoutJob
{
    private const int TabSpaces = 4;

    private readonly List<MarkedChar> _marked;
    private readonly TextGenerationSettings _settings;
    private readonly FontGlyphs _glyphs;

    private TextLayoutJob(List<MarkedChar> marked, in TextGenerationSettings settings, FontGlyphs glyphs)
    {
        _marked = marked;
        _settings = settings;
        _glyphs = glyphs;
        FontSize = settings.fontSize > 0 ? settings.fontSize : settings.font.fontSize;
    }

    public Font Font => _settings.font;

    /// <summary>The generation of the font's atlas the last layout drew into.</summary>
    public int AtlasGeneration => _glyphs.AtlasGeneration;

    /// <summary>The font size before the scale factor, without best fit.</summary>
    public int FontSize { get; }

    private Vector2 Extents => new(_settings.generationExtents.x * _settings.scaleFactor, _settings.generationExtents.y * _settings.scaleFactor);

    private bool Wraps => _settings.horizontalOverflow == HorizontalWrapMode.Wrap;

    /// <summary>The job for the text, or null when the settings have no font the engine can draw.</summary>
    public static TextLayoutJob? For(string text, in TextGenerationSettings settings)
    {
        if (settings.font is not { } font || font.Glyphs is not { } glyphs || !glyphs.TryFace(out _)) return null;
        return new TextLayoutJob(TextMarkup.Parse(text, settings.richText, settings.color.a), settings, glyphs);
    }

    private int Scaled(int size) => Math.Max(1, (int)MathF.Round(size * _settings.scaleFactor));

    private List<LayoutChar> Measure(int baseSize)
    {
        var chars = new List<LayoutChar>(_marked.Count);
        foreach (var marked in _marked)
        {
            if (marked.Code == '\r') continue;
            var size = marked.Size > 0 ? Scaled(marked.Size) : baseSize;
            chars.Add(Characterize(marked.Code, new CharStyle(size, _settings.fontStyle | marked.Style, marked.Color ?? _settings.color)));
        }
        return Kern(chars);
    }

    /// <summary>Gives each glyph the kerning against the glyph before it, when both are glyphs of one size.</summary>
    private List<LayoutChar> Kern(List<LayoutChar> chars)
    {
        var pairs = new List<(int Size, uint Left, uint Right)>();
        for (var i = 1; i < chars.Count; i++)
            if (Pairs(chars[i - 1], chars[i])) pairs.Add((chars[i].Style.Size, chars[i - 1].Glyph, chars[i].Glyph));
        foreach (var size in pairs.Select(pair => pair.Size).Distinct())
            _glyphs.Kerning.Prepare(size, pairs.Where(pair => pair.Size == size).Select(pair => (pair.Left, pair.Right)));
        for (var i = 1; i < chars.Count; i++)
            if (Pairs(chars[i - 1], chars[i]))
                chars[i] = chars[i] with { Kern = _glyphs.Kerning.Adjustment(chars[i].Style.Size, chars[i - 1].Glyph, chars[i].Glyph) };
        return chars;
    }

    private static bool Pairs(LayoutChar left, LayoutChar right) => left.Glyph != 0 && right.Glyph != 0 && left.Style.Size == right.Style.Size;

    private LayoutChar Characterize(int code, CharStyle style)
    {
        if (code == '\n') return new LayoutChar(code, 0, 0, style);
        if (code == '\t') return new LayoutChar(code, 0, TabSpaces * Advance(_glyphs.GlyphIndex(' '), style.Size), style);
        var glyph = _glyphs.GlyphIndex(code);
        return new LayoutChar(code, glyph, glyph == 0 ? 0 : Advance(glyph, style.Size), style);
    }

    private int Advance(uint glyph, int size) =>
        glyph != 0 && _glyphs.TryMetrics(size, glyph, out var metrics) ? (int)MathF.Round(metrics.horizontalAdvance) : 0;

    /// <summary>The characters at a base size broken into lines, with each line measured at its largest character size; a
    /// line with no characters takes the size of the character that ended it.</summary>
    private Arranged Arrange(int baseSize, bool wrap)
    {
        var chars = Measure(baseSize);
        var lines = new TextLayoutBreaker(chars, Extents.x, wrap).Lines();
        var measures = new List<LineMeasures>(lines.Count);
        foreach (var line in lines)
        {
            var size = line.End > line.Start ? chars.Skip(line.Start).Take(line.End - line.Start).Max(c => c.Style.Size) : line.End < chars.Count ? chars[line.End].Style.Size : baseSize;
            var (ascent, height) = _glyphs.Lines(size);
            measures.Add(new LineMeasures(ascent, height, (int)MathF.Round(height * _settings.lineSpacing)));
        }
        return new Arranged(chars, lines, measures);
    }

    /// <summary>The widest line when the text does not wrap.</summary>
    public float PreferredWidth() => Arrange(Scaled(FontSize), false).Lines.Select(line => line.Width).DefaultIfEmpty(0).Max();

    /// <summary>The height of the block of lines when the text wraps as the settings say, with no truncation.</summary>
    public float PreferredHeight() => Arrange(Scaled(FontSize), Wraps).Height;

    /// <summary>The largest size from the minimum to the maximum at which the text, wrapped, has no line wider than the
    /// extents and is no taller than them, or the minimum when none does. The search is over metrics, so it adds no glyphs
    /// to the atlas.</summary>
    private int BestFitSize()
    {
        var min = Math.Max(1, _settings.resizeTextMinSize);
        var (low, high, best) = (min, Math.Max(min, _settings.resizeTextMaxSize), min);
        while (low <= high)
        {
            var size = (low + high) / 2;
            if (FitsAt(size)) (best, low) = (size, size + 1);
            else high = size - 1;
        }
        return best;
    }

    private bool FitsAt(int size)
    {
        var arranged = Arrange(Scaled(size), true);
        var extents = Extents;
        return arranged.Height <= extents.y && arranged.Lines.All(line => line.Width <= extents.x);
    }

    /// <summary>Lays the text out, drawing the glyphs of the kept lines into the font's atlas first so every UV refers to
    /// the final atlas.</summary>
    public TextLayoutResult Layout()
    {
        var size = _settings.resizeTextForBestFit ? BestFitSize() : FontSize;
        var arranged = Arrange(Scaled(size), Wraps);
        var extents = Extents;
        var area = new TextArea(-_settings.pivot.x * extents.x, (1 - _settings.pivot.y) * extents.y, extents,
            TextLayoutPlacer.AnchorFactors(_settings.textAnchor),
            _settings.verticalOverflow == VerticalWrapMode.Truncate && !_settings.generateOutOfBounds);
        var inks = arranged.Lines.Select(line => TextLayoutInk.Of(arranged.Chars, line, _glyphs, _settings.alignByGeometry)).ToList();
        var (placed, kept) = TextLayoutPlacer.Place(arranged.Lines, arranged.Measures, inks, area);
        var infos = DrawGlyphs(arranged.Chars, placed.Take(kept));
        var visible = kept < arranged.Lines.Count ? arranged.Lines[kept].Start : arranged.Chars.Count;
        var result = new TextLayoutResult(visible, size);
        TextLayoutEmitter.Emit(result, new EmitInput(arranged.Chars, placed, kept), infos);
        result.Extents = TextLayoutEmitter.Extents(placed.Take(kept));
        return result;
    }

    private Dictionary<GlyphKey, CharacterInfo> DrawGlyphs(IReadOnlyList<LayoutChar> chars, IEnumerable<PlacedLine> kept)
    {
        var codes = new Dictionary<GlyphKey, int>();
        foreach (var line in kept)
            for (var i = line.Line.Start; i < line.Line.End; i++)
                if (chars[i].Glyph != 0) codes.TryAdd(new GlyphKey(chars[i].Glyph, chars[i].Style.Size, chars[i].Style.Style), chars[i].Code);
        var infos = new Dictionary<GlyphKey, CharacterInfo>();
        if (codes.Count == 0) return infos;
        _glyphs.EnsureGlyphs(codes.Keys);
        foreach (var (key, code) in codes)
            if (_glyphs.TryInfo(code, key, out var info)) infos[key] = info;
        return infos;
    }
}
