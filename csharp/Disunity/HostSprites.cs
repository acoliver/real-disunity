using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly Dictionary<ulong, WeakReference<Sprite>> SpriteContent = new();
    // Names outlive the Sprite object, which the cache holds weakly, so a sprite read back later keeps its name.
    private static readonly Dictionary<ulong, string> SpriteNames = new();
    internal static Sprite ResolveSprite(DisunitySprite value)
    {
        if (!SpriteContent.TryGetValue(value.identity, out var weak) || !weak.TryGetTarget(out var sprite))
        {
            sprite = new Sprite(value);
            if (SpriteNames.TryGetValue(value.identity, out var name)) sprite.name = name;
            SpriteContent[value.identity] = new WeakReference<Sprite>(sprite);
        }
        return sprite;
    }

    /// <summary>Names the sprite and every later object that stands for its content.</summary>
    internal static void NameSprite(Sprite sprite, string name)
    {
        sprite.name = name;
        SpriteNames[sprite.Description.identity] = name;
    }

    // Sprite identities to what a sprite over a managed texture has beyond the native content. Like the names, they
    // outlive the Sprite object so a sprite read back from a renderer is complete.
    private static readonly Dictionary<ulong, SpriteDetails> SpriteDetailTable = new();
    private static readonly Dictionary<ulong, WeakReference<Texture2D>> PlayerTextures = new();
    private static ulong _nextPlayerTexture = FirstPlayerTextureIdentity;

    /// <summary>The renderer identity of the first player texture. Scene textures count up from the identity the
    /// scene load is given (1 by default), so the two ranges meet only after 2^40 scene textures; this is the
    /// engine's choice. A texture takes its identity the first time a sprite is made over it and keeps it.</summary>
    internal const ulong FirstPlayerTextureIdentity = 1UL << 40;

    internal static SpriteDetails? DetailsOf(ulong spriteIdentity)
    {
        lock (SpriteDetailTable) return SpriteDetailTable.GetValueOrDefault(spriteIdentity);
    }

    internal static ulong AllocatePlayerTextureIdentity(Texture2D texture)
    {
        lock (PlayerTextures)
        {
            var identity = _nextPlayerTexture++;
            PlayerTextures[identity] = new WeakReference<Texture2D>(texture);
            return identity;
        }
    }

    /// <summary>The player texture that has this renderer identity, while it is alive.</summary>
    internal static Texture2D? PlayerTexture(ulong identity)
    {
        lock (PlayerTextures)
            return PlayerTextures.TryGetValue(identity, out var weak) && weak.TryGetTarget(out var texture) ? texture : null;
    }

    public static Sprite CreateSprite(ulong textureIdentity, uint textureWidth, uint textureHeight,
        Rect rect, Vector2 pivot, float pixelsPerUnit = 100) =>
        ResolveSprite(Register(textureIdentity, textureWidth, textureHeight, rect, pivot, pixelsPerUnit));

    /// <summary>Makes a sprite over a managed texture. Its native content covers the rectangle its pixels are in.</summary>
    internal static Sprite CreateSprite(Texture2D texture, SpriteDetails details, Vector2 pivot, float pixelsPerUnit)
    {
        var placed = details.Packed ? details.TextureRect : details.Rect;
        var value = Register(texture.RendererIdentity, (uint)texture.width, (uint)texture.height, placed, pivot, pixelsPerUnit);
        var sprite = new Sprite(value, texture);
        lock (SpriteDetailTable) SpriteDetailTable[value.identity] = details;
        SpriteContent[value.identity] = new WeakReference<Sprite>(sprite);
        return sprite;
    }

    private static unsafe DisunitySprite Register(ulong textureIdentity, uint textureWidth, uint textureHeight,
        Rect rect, Vector2 pivot, float pixelsPerUnit)
    {
        EnsureLoaded();
        var value = new DisunitySprite { present = 1, texture = textureIdentity, width = textureWidth, height = textureHeight,
            x = rect.x, y = rect.y, w = rect.width, h = rect.height, pivot_x = pivot.x, pivot_y = pivot.y, pixels_per_unit = pixelsPerUnit };
        ThrowIfError(NativeBindings.disunity_sprite_register(&value));
        return value;
    }
}
