using UnityEngine.TextCore;

namespace UnityEngine;

/// <summary>
/// How one character of a font draws at one size and style: its advance, the box of its ink relative to the pen
/// position and baseline, and where that ink sits in the font's atlas texture. Lengths are whole pixels, so text at scale
/// 1 sits on whole pixels; the UVs run from the bottom row up, as the atlas stores its rows.
/// </summary>
public struct CharacterInfo
{
    public int index;
    public int size;
    public FontStyle style;
    public int advance;
    public int glyphWidth;
    public int glyphHeight;
    public int bearing;
    public int minX;
    public int maxX;
    public int minY;
    public int maxY;
    public Vector2 uvBottomLeft;
    public Vector2 uvBottomRight;
    public Vector2 uvTopRight;
    public Vector2 uvTopLeft;

    /// <summary>The info of a glyph that has its metrics at <paramref name="size"/> and its rectangle in an atlas of the
    /// given size. The ink box starts at the rounded bearing, with the rectangle's size, so rounding moves ink by less than
    /// one pixel. A glyph without ink has an empty rectangle and collapsed UVs.</summary>
    internal static CharacterInfo Of(int index, int size, FontStyle style, Glyph glyph, int atlasWidth, int atlasHeight)
    {
        var rect = glyph.glyphRect;
        var minX = (int)MathF.Round(glyph.metrics.horizontalBearingX);
        var maxY = (int)MathF.Round(glyph.metrics.horizontalBearingY);
        var (u0, v0) = (rect.x / (float)atlasWidth, rect.y / (float)atlasHeight);
        var (u1, v1) = ((rect.x + rect.width) / (float)atlasWidth, (rect.y + rect.height) / (float)atlasHeight);
        return new CharacterInfo
        {
            index = index, size = size, style = style, advance = (int)MathF.Round(glyph.metrics.horizontalAdvance),
            glyphWidth = rect.width, glyphHeight = rect.height, bearing = minX,
            minX = minX, maxX = minX + rect.width, maxY = maxY, minY = maxY - rect.height,
            uvBottomLeft = new Vector2(u0, v0), uvBottomRight = new Vector2(u1, v0),
            uvTopRight = new Vector2(u1, v1), uvTopLeft = new Vector2(u0, v1),
        };
    }
}
