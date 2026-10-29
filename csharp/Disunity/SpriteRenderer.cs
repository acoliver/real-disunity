using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// Immutable sprite content descriptor and an <see cref="Object"/>, so a game can name it, find it by
/// instance ID and ask <c>Resources</c> for it. Its name is the one the scene's plan gives it, or empty
/// for a sprite made without one. Renderer state lives in the native world.
/// </summary>
public sealed class Sprite : Object
{
    internal readonly DisunitySprite Description;
    internal Sprite(DisunitySprite value) { Description = value; }
    internal Sprite Copy() => new(Description) { name = name };
    public Rect rect => new(Description.x, Description.y, Description.w, Description.h);
    public float pixelsPerUnit => Description.pixels_per_unit;
    public Vector2 pivot => new(Description.pivot_x * Description.w, Description.pivot_y * Description.h);
    public Bounds bounds => new(new((0.5f - Description.pivot_x) * Description.w / pixelsPerUnit,
        (0.5f - Description.pivot_y) * Description.h / pixelsPerUnit, 0),
        new(Description.w / pixelsPerUnit, Description.h / pixelsPerUnit, 0));
}
public sealed class SpriteRenderer : NativeComponent
{
    private unsafe DisunitySprite Read()
    {
        DisunitySprite value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_sprite_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Write(DisunitySprite value) => DisunityHost.ThrowIfError(
        NativeBindings.disunity_sprite_write(DisunityHost.WorldHandle, Handle, &value));
    public Material sharedMaterial
    {
        get => Material.FromNative(Read().lighting);
        set { ArgumentNullException.ThrowIfNull(value); var v = Read(); v.lighting = (uint)value.SpriteMaterial; Write(v); }
    }
    public Sprite? sprite
    {
        get { var value = Read(); return value.present == 0 ? null : DisunityHost.ResolveSprite(value); }
        set
        {
            var current = Read();
            var content = value?.Description ?? default;
            current.present = value == null ? 0u : 1u;
            current.identity = content.identity;
            current.texture = content.texture; current.width = content.width; current.height = content.height;
            current.x = content.x; current.y = content.y; current.w = content.w; current.h = content.h;
            current.pivot_x = content.pivot_x; current.pivot_y = content.pivot_y; current.pixels_per_unit = content.pixels_per_unit;
            Write(current);
        }
    }
    public Color color { get { var v = Read(); return new(v.r, v.g, v.b, v.a); } set { var v = Read(); v.r = value.r; v.g = value.g; v.b = value.b; v.a = value.a; Write(v); } }
    public bool enabled { get => Read().enabled == 1; set { var v = Read(); v.enabled = value ? 1u : 0u; Write(v); } }
    public bool flipX { get => Read().flip_x == 1; set { var v = Read(); v.flip_x = value ? 1u : 0u; Write(v); } }
    public bool flipY { get => Read().flip_y == 1; set { var v = Read(); v.flip_y = value ? 1u : 0u; Write(v); } }
    public int sortingOrder { get => Read().sorting_order; set { var v = Read(); v.sorting_order = value; Write(v); } }
    public int sortingLayerID { get => Read().sorting_layer; set { var v = Read(); v.sorting_layer = value; Write(v); } }
    public int sortingLayer { get => sortingLayerID; set => sortingLayerID = value; }
    public string sortingLayerName { get => sortingLayerID == 0 ? "Default" : throw new NotSupportedException("named sorting layer is not registered"); set { if (value != "Default") throw new NotSupportedException("named sorting layer is not registered"); sortingLayerID = 0; } }
    public unsafe Bounds bounds
    {
        get
        {
            DisunityBounds v;
            DisunityHost.ThrowIfError(NativeBindings.disunity_sprite_bounds(DisunityHost.WorldHandle, Handle, &v));
            return new(new((v.min_x + v.max_x) / 2, (v.min_y + v.max_y) / 2, (v.min_z + v.max_z) / 2),
                new(v.max_x - v.min_x, v.max_y - v.min_y, v.max_z - v.min_z));
        }
    }
}
