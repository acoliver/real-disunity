using System.Text;

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

/// <summary>
/// One layout of a string with one generation settings value: it measures the characters, breaks them into lines, places
/// the lines and turns them into vertices. All lengths are in pixels at the settings' scale factor, measured from the
/// pivot with y growing upward. Characters and line breaks need only the face's metrics; the atlas is touched only by
/// <see cref="Layout"/>, so the preferred sizes never add glyphs.
/// </summary>
internal sealed class TextLayoutJob
{
    private const int TabSpaces = 4;

    private readonly string _text;
    private readonly TextGenerationSettings _settings;
    private readonly FontGlyphs _glyphs;
    private readonly int _size;
    private readonly List<LayoutChar> _chars = [];

    private TextLayoutJob(string text, in TextGenerationSettings settings, FontGlyphs glyphs)
    {
        _text = text;
        _settings = settings;
        _glyphs = glyphs;
        var requested = settings.fontSize > 0 ? settings.fontSize : settings.font.fontSize;
        FontSize = requested;
        _size = Math.Max(1, (int)MathF.Round(requested * settings.scaleFactor));
    }

    public Font Font => _settings.font;

    /// <summary>The generation of the font's atlas the last layout drew into.</summary>
    public int AtlasGeneration => _glyphs.AtlasGeneration;

    /// <summary>The font size before the scale factor.</summary>
    public int FontSize { get; }

    private Vector2 Extents => new(_settings.generationExtents.x * _settings.scaleFactor, _settings.generationExtents.y * _settings.scaleFactor);

    /// <summary>The job for the text, or null when the settings have no font the engine can draw. Settings the layout does
    /// not handle yet are rejected by name.</summary>
    public static TextLayoutJob? For(string text, in TextGenerationSettings settings)
    {
        if (settings.richText) throw Unsupported("richText");
        if (settings.resizeTextForBestFit) throw Unsupported("resizeTextForBestFit");
        if (settings.alignByGeometry) throw Unsupported("alignByGeometry");
        if (settings.font is not { } font || font.Glyphs is not { } glyphs || !glyphs.TryFace(out _)) return null;
        var job = new TextLayoutJob(text, settings, glyphs);
        job.Measure();
        return job;
    }

    private static NotSupportedException Unsupported(string member) =>
        new($"TextGenerator cannot lay out text with {member} yet: the engine has no layout for it");

    private void Measure()
    {
        var space = Advance(' ');
        foreach (var rune in _text.EnumerateRunes())
        {
            if (rune.Value == '\r') continue;
            if (rune.Value == '\n') _chars.Add(new LayoutChar('\n', 0, 0));
            else if (rune.Value == '\t') _chars.Add(new LayoutChar('\t', 0, TabSpaces * space));
            else
            {
                var glyph = _glyphs.GlyphIndex(rune.Value);
                _chars.Add(new LayoutChar(rune.Value, glyph, glyph == 0 ? 0 : Advance(rune.Value)));
            }
        }
    }

    private int Advance(int code) =>
        _glyphs.GlyphIndex(code) is var glyph && glyph != 0 && _glyphs.TryMetrics(_size, glyph, out var metrics) ? (int)MathF.Round(metrics.horizontalAdvance) : 0;

    private LineMeasures Measures()
    {
        var (ascent, height) = _glyphs.Lines(_size);
        return new LineMeasures(ascent, height, (int)MathF.Round(height * _settings.lineSpacing));
    }

    private List<TextLine> Break(bool wrap) => new TextLayoutBreaker(_chars, Extents.x, wrap).Lines();

    /// <summary>The widest line when the text does not wrap.</summary>
    public float PreferredWidth() => Break(false).Select(line => line.Width).DefaultIfEmpty(0).Max();

    /// <summary>The height of the block of lines when the text wraps as the settings say, with no truncation.</summary>
    public float PreferredHeight()
    {
        var lines = Break(_settings.horizontalOverflow == HorizontalWrapMode.Wrap);
        var measures = Measures();
        return lines.Count == 0 ? 0 : (lines.Count - 1) * measures.Step + measures.Height;
    }

    /// <summary>Lays the text out, drawing the glyphs of the kept lines into the font's atlas first so every UV refers to
    /// the final atlas.</summary>
    public TextLayoutResult Layout()
    {
        var lines = Break(_settings.horizontalOverflow == HorizontalWrapMode.Wrap);
        var measures = Measures();
        var extents = Extents;
        var area = new TextArea(-_settings.pivot.x * extents.x, (1 - _settings.pivot.y) * extents.y, extents,
            TextLayoutPlacer.AnchorFactors(_settings.textAnchor),
            _settings.verticalOverflow == VerticalWrapMode.Truncate && !_settings.generateOutOfBounds);
        var (placed, kept) = TextLayoutPlacer.Place(lines, measures, area);
        var infos = DrawGlyphs(placed.Take(kept));
        var visible = kept < lines.Count ? lines[kept].Start : _chars.Count;
        var result = new TextLayoutResult(visible, FontSize);
        var input = new EmitInput(_chars, placed, kept, measures, new GlyphStyle(_size, _settings.fontStyle, _settings.color));
        TextLayoutEmitter.Emit(result, input, infos);
        result.Extents = TextLayoutEmitter.Extents(placed.Take(kept), measures);
        return result;
    }

    private Dictionary<GlyphKey, CharacterInfo> DrawGlyphs(IEnumerable<PlacedLine> kept)
    {
        var codes = new Dictionary<GlyphKey, int>();
        foreach (var line in kept)
            for (var i = line.Line.Start; i < line.Line.End; i++)
                if (_chars[i].Glyph != 0) codes.TryAdd(new GlyphKey(_chars[i].Glyph, _size, _settings.fontStyle), _chars[i].Code);
        var infos = new Dictionary<GlyphKey, CharacterInfo>();
        if (codes.Count == 0) return infos;
        _glyphs.EnsureGlyphs(codes.Keys);
        foreach (var (key, code) in codes)
            if (_glyphs.TryInfo(code, key, out var info)) infos[key] = info;
        return infos;
    }
}
