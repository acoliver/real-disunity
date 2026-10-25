using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly Dictionary<ulong, WeakReference<Sprite>> SpriteContent = new();
    internal static Sprite ResolveSprite(DisunitySprite value)
    {
        if (!SpriteContent.TryGetValue(value.identity, out var weak) || !weak.TryGetTarget(out var sprite))
        {
            sprite = new Sprite(value);
            SpriteContent[value.identity] = new WeakReference<Sprite>(sprite);
        }
        return sprite;
    }

    public static unsafe Sprite CreateSprite(ulong textureIdentity, uint textureWidth, uint textureHeight,
        Rect rect, Vector2 pivot, float pixelsPerUnit = 100)
    {
        EnsureLoaded();
        var value = new DisunitySprite { present = 1, texture = textureIdentity, width = textureWidth, height = textureHeight,
            x = rect.x, y = rect.y, w = rect.width, h = rect.height, pivot_x = pivot.x, pivot_y = pivot.y, pixels_per_unit = pixelsPerUnit };
        ThrowIfError(NativeBindings.disunity_sprite_register(&value));
        return ResolveSprite(value);
    }
}
