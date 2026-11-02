using Disunity.Native;

namespace UnityEngine.TextCore.LowLevel;

public static unsafe partial class FontEngine
{
    /// <summary>Where and how a batch of glyphs is drawn: the packing state the caller passed and the texture it owns.</summary>
    private readonly record struct AtlasTarget(
        int Padding, GlyphPackingMode Packing, List<GlyphRect> Free, List<GlyphRect> Used, GlyphRenderMode Mode, Texture2D Texture);

    /// <summary>
    /// Clears an atlas texture to transparent before glyphs are drawn into it: every byte of its pixel data is set to
    /// zero, and the renderer sees the change with the next <see cref="Texture2D.Apply()"/>.
    /// </summary>
    public static void ResetAtlasTexture(Texture2D texture)
    {
        ArgumentNullException.ThrowIfNull(texture);
        texture.PixelBytes.Clear();
        texture.DistanceSpread = 0;
    }

    /// <summary>
    /// Draws one glyph into the atlas texture: its rectangle (the glyph's size rounded up, plus <c>padding</c> on every
    /// side) is taken from <c>freeGlyphRects</c> by the packing mode, moved to <c>usedGlyphRects</c>, and filled with
    /// coverage (the <c>SMOOTH</c> and <c>RASTER</c> modes) or a signed distance field spread over the padding (the
    /// <c>SDF</c> modes). The glyph's own <c>glyphRect</c> is that rectangle without the padding, the place of the
    /// glyph's pixels, as a font asset stores it; text adds the padding back around it when it draws. A glyph with
    /// nothing to draw, such as a space, takes no space and gets an empty rectangle. Returns false, changing nothing, when there is no face, no such glyph, or no room.
    /// </summary>
    public static bool TryAddGlyphToTexture(uint glyphIndex, int padding, GlyphPackingMode packingMode,
        List<GlyphRect> freeGlyphRects, List<GlyphRect> usedGlyphRects, GlyphRenderMode renderMode, Texture2D texture, out Glyph? glyph)
    {
        var target = Target(padding, packingMode, freeGlyphRects, usedGlyphRects, renderMode, texture, nameof(TryAddGlyphToTexture));
        glyph = TryAdd(glyphIndex, target);
        Publish(target, glyph is { glyphRect.width: > 0 });
        return glyph != null;
    }

    /// <summary>Adds the glyphs in order and returns true when all of them fit. <paramref name="glyphs"/> holds those that were added.</summary>
    public static bool TryAddGlyphsToTexture(List<uint> glyphIndexes, int padding, GlyphPackingMode packingMode,
        List<GlyphRect> freeGlyphRects, List<GlyphRect> usedGlyphRects, GlyphRenderMode renderMode, Texture2D texture, out Glyph[] glyphs)
    {
        ArgumentNullException.ThrowIfNull(glyphIndexes);
        var target = Target(padding, packingMode, freeGlyphRects, usedGlyphRects, renderMode, texture, nameof(TryAddGlyphsToTexture));
        var added = new List<Glyph>();
        foreach (var index in glyphIndexes)
            if (TryAdd(index, target) is { } glyph) added.Add(glyph);
        glyphs = added.ToArray();
        Publish(target, glyphs.Any(glyph => glyph.glyphRect.width > 0));
        return glyphs.Length == glyphIndexes.Count;
    }

    /// <summary>
    /// Adds the glyphs of <paramref name="glyphsToAdd"/> in order. Each one that is drawn moves to
    /// <paramref name="glyphsAdded"/> with its metrics and rectangle set; those that do not fit stay in
    /// <paramref name="glyphsToAdd"/>. Returns true when none stayed.
    /// </summary>
    public static bool TryAddGlyphsToTexture(List<Glyph> glyphsToAdd, List<Glyph> glyphsAdded, int padding, GlyphPackingMode packingMode,
        List<GlyphRect> freeGlyphRects, List<GlyphRect> usedGlyphRects, GlyphRenderMode renderMode, Texture2D texture)
    {
        ArgumentNullException.ThrowIfNull(glyphsToAdd);
        ArgumentNullException.ThrowIfNull(glyphsAdded);
        var target = Target(padding, packingMode, freeGlyphRects, usedGlyphRects, renderMode, texture, nameof(TryAddGlyphsToTexture));
        glyphsAdded.Clear();
        for (var i = 0; i < glyphsToAdd.Count;)
        {
            if (TryAdd(glyphsToAdd[i].index, target) is not { } drawn) { i++; continue; }
            glyphsToAdd[i].metrics = drawn.metrics;
            glyphsToAdd[i].glyphRect = drawn.glyphRect;
            glyphsAdded.Add(glyphsToAdd[i]);
            glyphsToAdd.RemoveAt(i);
        }
        Publish(target, glyphsAdded.Any(glyph => glyph.glyphRect.width > 0));
        return glyphsToAdd.Count == 0;
    }

    private static AtlasTarget Target(int padding, GlyphPackingMode packing, List<GlyphRect> free, List<GlyphRect> used,
        GlyphRenderMode mode, Texture2D texture, string member)
    {
        ArgumentNullException.ThrowIfNull(free);
        ArgumentNullException.ThrowIfNull(used);
        ArgumentNullException.ThrowIfNull(texture);
        if (padding < 0) throw new ArgumentOutOfRangeException(nameof(padding), padding, "padding cannot be negative");
        if (texture.format is not (TextureFormat.Alpha8 or TextureFormat.R8))
            throw new NotSupportedException($"FontEngine.{member}: the atlas texture is {texture.format}, but glyphs are drawn only into Alpha8 or R8 textures");
        if (mode is GlyphRenderMode.COLOR or GlyphRenderMode.COLOR_HINTED)
            throw new NotSupportedException($"FontEngine.{member}: color glyphs ({mode}) are not drawn");
        return new AtlasTarget(padding, packing, free, used, mode, texture);
    }

    private static bool IsDistanceField(GlyphRenderMode mode) =>
        mode is GlyphRenderMode.SDF or GlyphRenderMode.SDF8 or GlyphRenderMode.SDF16 or GlyphRenderMode.SDF32
            or GlyphRenderMode.SDFAA or GlyphRenderMode.SDFAA_HINTED;

    /// <summary>The drawn glyph, or null when it cannot be drawn.</summary>
    private static Glyph? TryAdd(uint index, AtlasTarget target)
    {
        if (GlyphOf(index) is not { } glyph) return null;
        var metrics = glyph.metrics;
        if (metrics.width <= 0 || metrics.height <= 0) return glyph;
        var width = (int)Math.Ceiling(metrics.width) + 2 * target.Padding;
        var height = (int)Math.Ceiling(metrics.height) + 2 * target.Padding;
        var texture = target.Texture;
        var free = target.Free.ToArray();
        if (!FontAtlasPacker.TryPlace(target.Packing, width, height, texture.width, texture.height, target.Free, target.Used, out var rect)) return null;
        if (!Render(index, rect, target))
        {
            target.Used.RemoveAt(target.Used.Count - 1);
            target.Free.Clear();
            target.Free.AddRange(free);
            return null;
        }
        glyph.glyphRect = new GlyphRect(rect.x + target.Padding, rect.y + target.Padding, rect.width - 2 * target.Padding, rect.height - 2 * target.Padding);
        return glyph;
    }

    /// <summary>Hands the texture to the renderer once a batch has drawn into it, marked with the spread of the distance
    /// fields drawn (the padding, at least one texel as the glyph drawing spreads it) or as coverage.</summary>
    private static void Publish(AtlasTarget target, bool drew)
    {
        if (!drew) return;
        target.Texture.DistanceSpread = IsDistanceField(target.Mode) ? (uint)Math.Max(1, target.Padding) : 0;
        target.Texture.Apply(false);
    }

    private static bool Render(uint index, GlyphRect rect, AtlasTarget target)
    {
        var texture = target.Texture;
        var render = new DisunityGlyphRender
        {
            glyph = index, mode = IsDistanceField(target.Mode) ? 1u : 0u, padding = (uint)target.Padding,
            x = (uint)rect.x, y = (uint)rect.y, width = (uint)rect.width, height = (uint)rect.height,
            atlas_width = (uint)texture.width, atlas_height = (uint)texture.height,
        };
        var pixels = texture.PixelBytes;
        fixed (byte* atlas = pixels)
            return NativeBindings.disunity_face_glyph_render(_faceWorld, _face, &render, atlas, (uint)pixels.Length).code == 0;
    }
}
