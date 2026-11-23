using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;

namespace UnityEngine;

/// <summary>
/// The glyph source of one dynamic <see cref="Font"/>: a face of its own in the loaded world, loaded from the font's file
/// on first use, with the glyph measurements the layout asks for and the atlas those glyphs are drawn into. The face
/// belongs to the world; when the world changes the face, the measurements and the atlas are dropped and made again on
/// the next request, and the font announces the new atlas. Measurements are cached per pixel size, so laying out the
/// same text twice makes no native call.
/// </summary>
internal sealed class FontGlyphs
{
    private const float LoadSize = 16;

    private readonly Font _font;
    private readonly byte[] _data;
    private readonly Dictionary<(int Size, uint Glyph), GlyphMetrics> _metrics = new();
    private readonly Dictionary<int, (int Ascent, int LineHeight)> _lines = new();
    private readonly HashSet<(int Code, int Size, FontStyle Style)> _requested = [];
    private readonly List<(int Code, int Size, FontStyle Style)> _requestedInOrder = [];
    private FaceHandle _face;
    private bool _hasFace;
    private int _size;
    private FontGlyphAtlas? _atlas;
    private bool _hadAtlas;
    private bool _rebuilt;

    /// <summary>Changes whenever the atlas is made, replaced or dropped, which moves glyph rectangles, so a layout built
    /// from earlier UVs knows to run again.</summary>
    public int AtlasGeneration { get; private set; }

    public FontGlyphs(Font font, byte[] data)
    {
        _font = font;
        _data = data;
        Kerning = new FontKerning(this);
    }

    /// <summary>The kerning of the face, dropped with it when the world changes.</summary>
    public FontKerning Kerning { get; }

    /// <summary>The loaded face, loaded now if the world changed since the last call; false with no world or a file the
    /// engine cannot read.</summary>
    public bool TryFace(out FaceHandle face)
    {
        if (_hasFace && _face.InLoadedWorld)
        {
            face = _face;
            return true;
        }
        Drop();
        _hasFace = FaceHandle.TryLoad(_data, LoadSize, 0, out _face);
        _size = (int)LoadSize;
        face = _face;
        return _hasFace;
    }

    private void Drop()
    {
        _hasFace = false;
        _metrics.Clear();
        _lines.Clear();
        Kerning.Clear();
        _requested.Clear();
        _requestedInOrder.Clear();
        if (_atlas is null) return;
        _atlas = null;
        AtlasGeneration++;
        _font.AdoptAtlas(null);
    }

    /// <summary>Sets the face to a pixel size and returns it; false when there is no face.</summary>
    public bool TryUseSize(int size, out FaceHandle face)
    {
        if (!TryFace(out face)) return false;
        if (_size == size) return true;
        _size = size;
        return face.SetSize(size);
    }

    public uint GlyphIndex(int codePoint) => codePoint >= 0 && TryFace(out var face) ? face.GlyphIndex((uint)codePoint) : 0;

    /// <summary>The metrics of a glyph at a pixel size, cached; false when the glyph does not exist.</summary>
    public bool TryMetrics(int size, uint glyph, out GlyphMetrics metrics)
    {
        if (_metrics.TryGetValue((size, glyph), out metrics)) return true;
        if (!TryUseSize(size, out var face) || FontEngine.GlyphOf(face, glyph) is not { } found) return false;
        metrics = found.metrics;
        _metrics[(size, glyph)] = metrics;
        return true;
    }

    /// <summary>The rounded ascent and line height at a pixel size, cached; zero with no face.</summary>
    public (int Ascent, int LineHeight) Lines(int size)
    {
        if (_lines.TryGetValue(size, out var cached)) return cached;
        if (!TryUseSize(size, out var face) || !face.TryInfo(out var info)) return default;
        return _lines[size] = ((int)MathF.Round(info.ascent_line), (int)MathF.Round(info.line_height));
    }

    /// <summary>Draws the glyphs into the atlas, making it first if needed, and notes whether the texture was replaced so
    /// the font can announce it. Returns false when there is no face.</summary>
    public bool EnsureGlyphs(IReadOnlyCollection<GlyphKey> keys)
    {
        if (!TryFace(out _)) return false;
        if (_atlas is null)
        {
            _atlas = new FontGlyphAtlas(this, $"{_font.name} atlas");
            _rebuilt |= _hadAtlas;
            _hadAtlas = true;
            AtlasGeneration++;
            _font.AdoptAtlas(_atlas.Texture);
        }
        if (_atlas.Ensure(keys))
        {
            _rebuilt = true;
            AtlasGeneration++;
            _font.AdoptAtlas(_atlas.Texture);
        }
        return true;
    }

    /// <summary>The info of a character at a size and style, drawn into the atlas now if it is not there; false when the
    /// font has no glyph for it or no room for it.</summary>
    public bool TryCharacter(int codePoint, int size, FontStyle style, out CharacterInfo info)
    {
        info = default;
        var glyph = GlyphIndex(codePoint);
        if (glyph == 0) return false;
        var key = new GlyphKey(glyph, size, style);
        EnsureGlyphs([key]);
        if (!TryInfo(codePoint, key, out info)) return false;
        if (_requested.Add((codePoint, size, style))) _requestedInOrder.Add((codePoint, size, style));
        return true;
    }

    /// <summary>The info of a glyph that is in the atlas now, for the character that maps to it.</summary>
    public bool TryInfo(int codePoint, GlyphKey key, out CharacterInfo info)
    {
        info = default;
        if (_atlas is null || !_atlas.TryGet(key, out var glyph)) return false;
        info = CharacterInfo.Of(codePoint, key.Size, key.Style, glyph, _atlas.Texture.width, _atlas.Texture.height);
        return true;
    }

    /// <summary>The infos of the requested characters whose glyphs are in the atlas, in the order they were first requested.</summary>
    public CharacterInfo[] Characters()
    {
        var infos = new List<CharacterInfo>();
        foreach (var (code, size, style) in _requestedInOrder)
            if (TryInfo(code, new GlyphKey(GlyphIndex(code), size, style), out var info))
                infos.Add(info);
        return infos.ToArray();
    }

    /// <summary>True once after the atlas was replaced or dropped since the last call.</summary>
    public bool TakeRebuilt()
    {
        var rebuilt = _rebuilt;
        _rebuilt = false;
        return rebuilt;
    }
}
