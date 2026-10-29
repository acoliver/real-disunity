using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// Immutable sprite content descriptor and an <see cref="Object"/>, so a game can name it, find it by
/// instance ID and ask <c>Resources</c> for it. Its name is the one the scene's plan gives it, or empty
/// for a sprite made without one. Renderer state lives in the native world.
/// </summary>
/// <remarks>
/// A sprite of a player's resources or made by <see cref="Create(Texture2D, Rect, Vector2)"/> keeps its managed
/// <see cref="Texture2D"/>; a sprite of the scene has none, so <see cref="texture"/> is null for it, its
/// <see cref="textureRect"/> is its <see cref="rect"/> and it has no border. A sprite is drawn as its rectangle:
/// polygon meshes, extrusion and the mesh type are not simulated. A packed sprite draws the rectangle of its
/// packed pixels, <see cref="textureRect"/>, while <see cref="rect"/> reports the rectangle it had before packing.
/// </remarks>
public sealed class Sprite : Object
{
    private const float DefaultPixelsPerUnit = 100;

    internal readonly DisunitySprite Description;
    private readonly Texture2D? _texture;

    internal Sprite(DisunitySprite value, Texture2D? texture = null)
    {
        Description = value;
        _texture = texture;
    }

    internal Sprite Copy() => new(Description, _texture) { name = name };

    private Rect NativeRect => new(Description.x, Description.y, Description.w, Description.h);
    private SpriteDetails? Details => DisunityHost.DetailsOf(Description.identity);

    public Rect rect => Details?.Rect ?? NativeRect;
    public float pixelsPerUnit => Description.pixels_per_unit;
    public Vector2 pivot => new(Description.pivot_x * rect.width, Description.pivot_y * rect.height);
    public Bounds bounds => new(new((0.5f - Description.pivot_x) * rect.width / pixelsPerUnit,
        (0.5f - Description.pivot_y) * rect.height / pixelsPerUnit, 0),
        new(rect.width / pixelsPerUnit, rect.height / pixelsPerUnit, 0));

    /// <summary>The texture the sprite is drawn from, or null for a sprite of the scene, whose texture the engine does not hold as an object.</summary>
    public Texture2D? texture => _texture ?? DisunityHost.PlayerTexture(Description.texture);

    /// <summary>Where the sprite's pixels are in <see cref="texture"/>.</summary>
    public Rect textureRect => Details?.TextureRect ?? NativeRect;

    /// <summary>The offset of <see cref="textureRect"/> within <see cref="rect"/>.</summary>
    public Vector2 textureRectOffset => Details?.TextureRectOffset ?? Vector2.zero;

    /// <summary>The nine-slice border in pixels: left, bottom, right, top.</summary>
    public Vector4 border => Details?.Border ?? Vector4.zero;

    /// <summary>Whether the sprite's pixels were packed into an atlas texture.</summary>
    public bool packed => Details?.Packed ?? false;

    public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot) =>
        Create(texture, rect, pivot, DefaultPixelsPerUnit);

    public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit) =>
        Create(texture, rect, pivot, pixelsPerUnit, 0);

    public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit, uint extrude) =>
        Create(texture, rect, pivot, pixelsPerUnit, extrude, SpriteMeshType.Tight);

    public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit, uint extrude, SpriteMeshType meshType) =>
        Create(texture, rect, pivot, pixelsPerUnit, extrude, meshType, Vector4.zero);

    public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit, uint extrude, SpriteMeshType meshType, Vector4 border) =>
        Create(texture, rect, pivot, pixelsPerUnit, extrude, meshType, border, false);

    /// <summary>Makes a sprite over <paramref name="rect"/> of <paramref name="texture"/>, whose pivot is a fraction of the rect.
    /// Extrusion, the mesh type and the fallback physics shape are accepted and not simulated.</summary>
    public static Sprite Create(Texture2D texture, Rect rect, Vector2 pivot, float pixelsPerUnit, uint extrude, SpriteMeshType meshType,
        Vector4 border, bool generateFallbackPhysicsShape)
    {
        ArgumentNullException.ThrowIfNull(texture);
        if (!float.IsFinite(pixelsPerUnit) || pixelsPerUnit <= 0)
            throw new ArgumentException($"Sprite.Create: pixelsPerUnit {pixelsPerUnit} must be finite and greater than zero");
        if (!float.IsFinite(pivot.x) || !float.IsFinite(pivot.y))
            throw new ArgumentException($"Sprite.Create: pivot {pivot} must be finite");
        if (!InsideTexture(texture, rect))
            throw new ArgumentException($"Sprite.Create: rect {rect} must have a positive size and lie inside the {texture.width}x{texture.height} texture");
        return DisunityHost.CreateSprite(texture, new SpriteDetails(rect, rect, Vector2.zero, border, false), pivot, pixelsPerUnit);
    }

    private static bool InsideTexture(Texture2D texture, Rect rect) =>
        float.IsFinite(rect.x) && float.IsFinite(rect.y) && float.IsFinite(rect.width) && float.IsFinite(rect.height)
        && rect.width > 0 && rect.height > 0 && rect.x >= 0 && rect.y >= 0
        && (double)rect.x + rect.width <= texture.width && (double)rect.y + rect.height <= texture.height;
}

/// <summary>What a sprite made over a managed texture has beyond the native content: the rectangle it
/// had before packing, where its pixels are, the offset of that rectangle, its border and whether it was packed.</summary>
internal sealed record SpriteDetails(Rect Rect, Rect TextureRect, Vector2 TextureRectOffset, Vector4 Border, bool Packed);
