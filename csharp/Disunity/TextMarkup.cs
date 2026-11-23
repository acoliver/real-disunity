using System.Globalization;
using System.Text;

namespace UnityEngine;

/// <summary>One character of text after its markup was read: its code point and the style the tags around it gave. A size
/// of 0 means the settings' size, no colour means the settings' colour, and the style holds the bold and italic bits the
/// tags added.</summary>
internal readonly record struct MarkedChar(int Code, int Size, Color? Color, FontStyle Style);

/// <summary>
/// Reads the rich text tags of a string: b, i, size=N (pixels before the scale factor) and color=VALUE (a hex colour or a
/// colour name that <see cref="ColorUtility"/> reads), each closed by its slash form. Only a tag that is valid and has its
/// partner is a tag. Any other text between angle brackets, a tag with a bad value, a tag never closed and a closing tag
/// never opened are characters like the rest. Tags nest per kind, and a closing tag ends the latest open tag of its kind.
/// </summary>
internal static class TextMarkup
{
    private enum Kind { Bold, Italic, Size, Color }

    private readonly record struct Tag(Kind Kind, bool Closing, int Start, int End, int Size, Color Color, bool HasAlpha);

    /// <summary>The characters of the text with the tags removed. With <paramref name="rich"/> false the whole string is
    /// characters. A colour tag without alpha takes <paramref name="alpha"/>.</summary>
    public static List<MarkedChar> Parse(string text, bool rich, float alpha)
    {
        var tags = rich ? Pair(Scan(text)) : [];
        var marked = new List<MarkedChar>(text.Length);
        var state = new Styles(alpha);
        var next = 0;
        var index = 0;
        while (index < text.Length)
        {
            if (next < tags.Count && tags[next].Start == index)
            {
                state.Apply(tags[next]);
                index = tags[next++].End;
                continue;
            }
            Rune.DecodeFromUtf16(text.AsSpan(index), out var rune, out var length);
            marked.Add(state.Mark(rune.Value));
            index += length;
        }
        return marked;
    }

    /// <summary>Every well-formed tag in the text, in order.</summary>
    private static List<Tag> Scan(string text)
    {
        var tags = new List<Tag>();
        for (var open = text.IndexOf('<'); open >= 0; open = text.IndexOf('<', open + 1))
        {
            var close = text.IndexOf('>', open + 1);
            if (close < 0) break;
            if (TryTag(text.AsSpan(open + 1, close - open - 1), open, close + 1, out var tag)) tags.Add(tag);
        }
        return tags;
    }

    private static bool TryTag(ReadOnlySpan<char> body, int start, int end, out Tag tag)
    {
        tag = default;
        var closing = body.Length > 0 && body[0] == '/';
        if (closing) body = body[1..];
        var equals = body.IndexOf('=');
        var name = equals < 0 ? body : body[..equals];
        if (KindOf(name) is not { } kind) return false;
        var plain = new Tag(kind, closing, start, end, 0, default, false);
        if (closing || kind is Kind.Bold or Kind.Italic)
        {
            tag = plain;
            return equals < 0;
        }
        var value = equals < 0 ? ReadOnlySpan<char>.Empty : body[(equals + 1)..];
        if (equals < 0) return false;
        if (kind == Kind.Size)
        {
            tag = plain with { Size = int.TryParse(value, NumberStyles.None, CultureInfo.InvariantCulture, out var size) ? size : 0 };
            return tag.Size > 0;
        }
        if (!ColorUtility.TryParseHtmlString(value.ToString(), out var color)) return false;
        tag = plain with { Color = color, HasAlpha = HasAlphaDigits(value) };
        return true;
    }

    private static Kind? KindOf(ReadOnlySpan<char> name) => name switch
    {
        "b" => Kind.Bold,
        "i" => Kind.Italic,
        "size" => Kind.Size,
        "color" => Kind.Color,
        _ => null,
    };

    private static bool HasAlphaDigits(ReadOnlySpan<char> value) => value.Length > 0 && value[0] == '#' && value.Length - 1 is 4 or 8;

    /// <summary>The tags that have a partner, in text order: each opening tag takes the next closing tag of its kind that
    /// no later opening tag of the same kind has taken.</summary>
    private static List<Tag> Pair(List<Tag> tags)
    {
        var open = new List<int>();
        var paired = new HashSet<int>();
        for (var i = 0; i < tags.Count; i++)
        {
            if (!tags[i].Closing) { open.Add(i); continue; }
            var match = open.FindLastIndex(o => tags[o].Kind == tags[i].Kind);
            if (match < 0) continue;
            paired.Add(open[match]);
            paired.Add(i);
            open.RemoveAt(match);
        }
        return tags.Where((_, i) => paired.Contains(i)).ToList();
    }

    /// <summary>The styles the tags open at one point of the text.</summary>
    private sealed class Styles(float alpha)
    {
        private int _bold;
        private int _italic;
        private readonly List<int> _sizes = [];
        private readonly List<Color> _colors = [];

        public void Apply(Tag tag)
        {
            switch (tag.Kind)
            {
                case Kind.Bold: _bold += tag.Closing ? -1 : 1; break;
                case Kind.Italic: _italic += tag.Closing ? -1 : 1; break;
                case Kind.Size: Push(_sizes, tag.Closing, tag.Size); break;
                default: Push(_colors, tag.Closing, tag.HasAlpha ? tag.Color : new Color(tag.Color.r, tag.Color.g, tag.Color.b, alpha)); break;
            }
        }

        private static void Push<T>(List<T> stack, bool closing, T value)
        {
            if (!closing) stack.Add(value);
            else if (stack.Count > 0) stack.RemoveAt(stack.Count - 1);
        }

        public MarkedChar Mark(int code) =>
            new(code, _sizes.Count > 0 ? _sizes[^1] : 0, _colors.Count > 0 ? _colors[^1] : null,
                (_bold > 0 ? FontStyle.Bold : FontStyle.Normal) | (_italic > 0 ? FontStyle.Italic : FontStyle.Normal));
    }
}
