namespace UnityEngine.Sprites;

/// <summary>
/// Reads the texture coordinates and sizes of a sprite that UI code lays its nine-slice meshes out from. UVs are
/// (xMin, yMin, xMax, yMax) as fractions of the texture. A sprite with no managed texture, such as a sprite of
/// the scene, has no texture size to divide by: the engine's choice is the full unit UV for both and zero padding.
/// </summary>
public static class DataUtility
{
    /// <summary>The UV of the sprite's whole pixel rectangle: its texture rect divided by the texture size.</summary>
    public static Vector4 GetOuterUV(Sprite sprite)
    {
        ArgumentNullException.ThrowIfNull(sprite);
        return sprite.texture is { } texture ? Uv(sprite.textureRect, texture) : new Vector4(0, 0, 1, 1);
    }

    /// <summary>The UV of the sprite's centre: the outer UV inset by the border. Where the sprite's rectangle has
    /// transparent padding around its pixels, the padding counts toward the border, so only the rest is inset.</summary>
    public static Vector4 GetInnerUV(Sprite sprite)
    {
        ArgumentNullException.ThrowIfNull(sprite);
        if (sprite.texture is not { } texture) return new Vector4(0, 0, 1, 1);
        var pixels = sprite.textureRect;
        var padding = GetPadding(sprite);
        var border = sprite.border;
        var inner = Rect.MinMaxRect(
            pixels.xMin + Math.Max(0, border.x - padding.x), pixels.yMin + Math.Max(0, border.y - padding.y),
            pixels.xMax - Math.Max(0, border.z - padding.z), pixels.yMax - Math.Max(0, border.w - padding.w));
        return Uv(inner, texture);
    }

    /// <summary>How far the sprite's pixels sit inside its rectangle: left, bottom, right, top. Zero for a sprite
    /// whose rectangle is its pixels.</summary>
    public static Vector4 GetPadding(Sprite sprite)
    {
        ArgumentNullException.ThrowIfNull(sprite);
        if (sprite.texture is null) return Vector4.zero;
        var rect = sprite.rect;
        var pixels = sprite.textureRect;
        var offset = sprite.textureRectOffset;
        return new Vector4(offset.x, offset.y, rect.width - pixels.width - offset.x, rect.height - pixels.height - offset.y);
    }

    /// <summary>The smallest size the sprite can be drawn at without its borders overlapping: the border sums.</summary>
    public static Vector2 GetMinSize(Sprite sprite)
    {
        ArgumentNullException.ThrowIfNull(sprite);
        var border = sprite.border;
        return new Vector2(border.x + border.z, border.y + border.w);
    }

    private static Vector4 Uv(Rect pixels, Texture2D texture) =>
        new(pixels.xMin / texture.width, pixels.yMin / texture.height, pixels.xMax / texture.width, pixels.yMax / texture.height);
}
