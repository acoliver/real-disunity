namespace UnityEngine;

/// <summary>How a character draws: the pixel size and style its glyph is drawn into the atlas at, and its colour.</summary>
internal readonly record struct CharStyle(int Size, FontStyle Style, Color32 Color);

/// <summary>One character the generator lays out: its code point, glyph index (0 for none), whole-pixel advance and style.
/// A newline has no glyph and no advance. <see cref="Kern"/> is the pixels added to the previous character's advance when
/// this one follows it on a line; it is 0 for the first character of a line.</summary>
internal readonly record struct LayoutChar(int Code, uint Glyph, int Advance, CharStyle Style)
{
    public int Kern { get; init; }
}

/// <summary>A line of laid-out characters: those from <see cref="Start"/> up to <see cref="End"/>, whose advances add up
/// to <see cref="Width"/>. The space or newline a line break consumes sits at <see cref="End"/> and is on no line.</summary>
internal readonly record struct TextLine(int Start, int End, int Width);

/// <summary>
/// Breaks characters into lines by the rule of the engine's native text: a newline ends a line, and when wrapping, a line
/// that grows wider than the limit breaks at its last space that is not its first character (the space is consumed), or
/// before the character that did not fit when it has no such space. The rest of the line is measured again as the start of
/// the next one, so a very long word breaks as often as it must.
/// </summary>
internal sealed class TextLayoutBreaker
{
    private readonly IReadOnlyList<LayoutChar> _chars;
    private readonly float _limit;
    private readonly bool _wrap;
    private readonly List<TextLine> _lines = [];

    public TextLayoutBreaker(IReadOnlyList<LayoutChar> chars, float limit, bool wrap)
    {
        _chars = chars;
        _limit = limit;
        _wrap = wrap;
    }

    /// <summary>The lines of the text: none for no characters, otherwise one per newline plus one.</summary>
    public List<TextLine> Lines()
    {
        if (_chars.Count == 0) return _lines;
        var start = 0;
        for (var i = 0; i <= _chars.Count; i++)
        {
            if (i < _chars.Count && _chars[i].Code != '\n') continue;
            BreakParagraph(start, i);
            start = i + 1;
        }
        return _lines;
    }

    private int Measure(int from, int to)
    {
        var width = 0;
        for (var i = from; i < to; i++) width += _chars[i].Advance + (i > from ? _chars[i].Kern : 0);
        return width;
    }

    private int LastSpace(int after, int through)
    {
        for (var i = through; i > after; i--)
            if (_chars[i].Code == ' ') return i;
        return -1;
    }

    private void BreakParagraph(int begin, int end)
    {
        var start = begin;
        if (_wrap)
        {
            var width = 0;
            for (var i = begin; i < end; i++)
            {
                width += _chars[i].Advance + (i > start ? _chars[i].Kern : 0);
                while (width > _limit && i > start)
                {
                    var space = LastSpace(start, i);
                    var (lineEnd, next) = space >= 0 ? (space, space + 1) : (i, i);
                    _lines.Add(new TextLine(start, lineEnd, Measure(start, lineEnd)));
                    start = next;
                    width = Measure(start, i + 1);
                }
            }
        }
        _lines.Add(new TextLine(start, end, Measure(start, end)));
    }
}
