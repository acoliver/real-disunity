namespace UnityEngine;

public sealed partial class Font
{
    private const int DefaultFontSize = 16;

    private FontGlyphs? _glyphs;

    /// <summary>The glyph source of a dynamic font, or null for a font with no file.</summary>
    internal FontGlyphs? Glyphs => FontData is { } data ? _glyphs ??= new FontGlyphs(this, data) : null;

    /// <summary>The size the font's own measurements are taken at, 16 pixels. Text asks for any size it needs.</summary>
    public int fontSize => DefaultFontSize;

    /// <summary>The distance in pixels from the baseline up to the top of the tallest line at <see cref="fontSize"/>;
    /// zero when the font has no usable file or no world is loaded.</summary>
    public int ascent => Glyphs?.Lines(fontSize).Ascent ?? 0;

    /// <summary>The distance in pixels between baselines at <see cref="fontSize"/>, zero as for <see cref="ascent"/>.</summary>
    public int lineHeight => Glyphs?.Lines(fontSize).LineHeight ?? 0;

    /// <summary>The font's name, which is its only name: the engine has no fallback font list.</summary>
    public string[] fontNames
    {
        get => [name];
        set => throw new NotSupportedException("Font.fontNames cannot be set: the engine has no fallback font list");
    }

    /// <summary>Whether the font has a glyph for the character.</summary>
    public bool HasCharacter(char c) => HasCharacter((int)c);

    public bool HasCharacter(int c) => Glyphs?.GlyphIndex(c) is > 0;

    public bool GetCharacterInfo(char ch, out CharacterInfo info) => GetCharacterInfo(ch, out info, 0, FontStyle.Normal);

    public bool GetCharacterInfo(char ch, out CharacterInfo info, int size) => GetCharacterInfo(ch, out info, size, FontStyle.Normal);

    /// <summary>Draws the character at the size and style into the font's atlas if it is not there and returns how it
    /// draws. Size 0 is <see cref="fontSize"/>. Bold and italic draw the regular face, which is the engine's choice, so
    /// each style has its own atlas entry and the same shape. False, with a default info, when the font has no file,
    /// there is no world, the font lacks the character or the atlas has no room for it.</summary>
    public bool GetCharacterInfo(char ch, out CharacterInfo info, int size, FontStyle style)
    {
        info = default;
        var found = Glyphs?.TryCharacter(ch, size > 0 ? size : fontSize, style, out info) == true;
        FlushTextureRebuilt();
        return found;
    }

    public void RequestCharactersInTexture(string characters) => RequestCharactersInTexture(characters, 0, FontStyle.Normal);

    public void RequestCharactersInTexture(string characters, int size) => RequestCharactersInTexture(characters, size, FontStyle.Normal);

    /// <summary>Draws each character of the string into the atlas, with a rebuild announced once at the end of the call.</summary>
    public void RequestCharactersInTexture(string characters, int size, FontStyle style)
    {
        ArgumentNullException.ThrowIfNull(characters);
        foreach (var ch in characters) Glyphs?.TryCharacter(ch, size > 0 ? size : fontSize, style, out _);
        FlushTextureRebuilt();
    }

    /// <summary>The characters requested so far that are in the atlas, each with the size and style it was requested at.
    /// The engine's fonts are dynamic, so the list cannot be set.</summary>
    public CharacterInfo[] characterInfo
    {
        get => Glyphs?.Characters() ?? [];
        set => throw new NotSupportedException("Font.characterInfo cannot be set: the engine draws its fonts on demand");
    }

    private bool _layoutFailureReported;

    /// <summary>True the first time it is called for this font, so a text generator logs why the font cannot be laid out once.</summary>
    internal bool ReportLayoutFailureOnce()
    {
        var first = !_layoutFailureReported;
        _layoutFailureReported = true;
        return first;
    }

    /// <summary>Raises <see cref="textureRebuilt"/> if the atlas was replaced during the call that ends here. The event
    /// runs last so a handler that rebuilds text never re-enters a half-finished layout.</summary>
    internal void FlushTextureRebuilt()
    {
        if (_glyphs?.TakeRebuilt() == true) RaiseTextureRebuilt();
    }
}
