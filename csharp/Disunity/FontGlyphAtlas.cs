using UnityEngine.TextCore;
using UnityEngine.TextCore.LowLevel;

namespace UnityEngine;

/// <summary>A glyph at one pixel size and style: the unit a font's atlas stores.</summary>
internal readonly record struct GlyphKey(uint Glyph, int Size, FontStyle Style);

/// <summary>
/// The Alpha8 coverage atlas a dynamic <see cref="Font"/> draws its glyphs into. It starts at 256 by 256 texels with one
/// texel of padding around each glyph. When a batch does not fit, every glyph is drawn again into a texture of twice the
/// side, up to 4096, and the old texture is destroyed. At the largest size the cached glyphs are dropped and only the
/// batch is drawn; a glyph that still does not fit is reported and left out. The glyph rectangles never move while the
/// texture stays.
/// </summary>
internal sealed class FontGlyphAtlas
{
    private const int Padding = 1;
    private const int FirstSide = 256;
    private const int MaxSide = 4096;

    private sealed class Page
    {
        public readonly Texture2D Texture;
        public readonly Dictionary<GlyphKey, Glyph> Glyphs = new();
        public readonly List<GlyphKey> Order = [];
        private readonly List<GlyphRect> _free;
        private readonly List<GlyphRect> _used = [];

        public Page(int side, string name)
        {
            Texture = new Texture2D(side, side, TextureFormat.Alpha8, false) { name = name, filterMode = FilterMode.Bilinear, wrapMode = TextureWrapMode.Clamp };
            _free = [new GlyphRect(0, 0, side, side)];
        }

        public bool TryDraw(FontGlyphs source, GlyphKey key)
        {
            if (Glyphs.ContainsKey(key)) return true;
            if (!source.TryUseSize(key.Size, out var face)) return false;
            var target = new FontEngine.AtlasTarget(Padding, GlyphPackingMode.BestShortSideFit, _free, _used, GlyphRenderMode.SMOOTH, Texture);
            if (FontEngine.TryAdd(face, key.Glyph, target) is not { } glyph) return false;
            Glyphs[key] = glyph;
            Order.Add(key);
            return true;
        }
    }

    private readonly FontGlyphs _source;
    private readonly string _name;
    private Page _page;

    public FontGlyphAtlas(FontGlyphs source, string name)
    {
        _source = source;
        _name = name;
        _page = new Page(FirstSide, name);
    }

    public Texture2D Texture => _page.Texture;

    public bool TryGet(GlyphKey key, out Glyph glyph) => _page.Glyphs.TryGetValue(key, out glyph!);

    /// <summary>Draws the glyphs the atlas lacks. Returns true when the texture was replaced to make room.</summary>
    public bool Ensure(IEnumerable<GlyphKey> wanted)
    {
        var missing = wanted.Where(key => !_page.Glyphs.ContainsKey(key)).Distinct().ToList();
        if (missing.Count == 0) return false;
        var fits = missing.All(key => _page.TryDraw(_source, key));
        _page.Texture.Apply(false);
        return !fits && Rebuild(missing);
    }

    private bool Rebuild(List<GlyphKey> missing)
    {
        var all = _page.Order.Concat(missing).ToList();
        for (var side = _page.Texture.width * 2; side <= MaxSide; side *= 2)
        {
            var page = new Page(side, _name);
            if (all.All(key => page.TryDraw(_source, key))) return Replace(page);
            Object.DestroyImmediate(page.Texture);
        }
        var last = new Page(MaxSide, _name);
        foreach (var key in missing.Where(key => !last.TryDraw(_source, key)))
            Debug.LogError($"Font atlas '{_name}' has no room for glyph {key.Glyph} at size {key.Size}");
        return Replace(last);
    }

    private bool Replace(Page page)
    {
        page.Texture.Apply(false);
        Object.DestroyImmediate(_page.Texture);
        _page = page;
        return true;
    }
}
