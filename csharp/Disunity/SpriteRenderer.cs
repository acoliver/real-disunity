using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>A <see cref="Renderer"/> that draws a <see cref="Sprite"/>. It has exactly one material, a built-in
/// sprite material: a list of any other length, or a material that is not one, throws. Its
/// <see cref="Renderer.localBounds"/> is the zero box; <see cref="bounds"/> is the world box of the sprite.</summary>
public sealed class SpriteRenderer : Renderer
{
    private DisunityMeshRenderer _other = Defaults();

    /// <summary>A sprite casts no shadow, receives none and uses no probes.</summary>
    private static DisunityMeshRenderer Defaults() => new()
    {
        enabled = 1,
        allow_occlusion_when_dynamic = 1,
        motion_vector_mode = (uint)MotionVectorGenerationMode.Object,
        rendering_layer_mask = 1,
    };

    private unsafe DisunitySprite Read()
    {
        DisunitySprite value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_sprite_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }
    private unsafe void Write(DisunitySprite value) => DisunityHost.ThrowIfError(
        NativeBindings.disunity_sprite_write(DisunityHost.WorldHandle, Handle, &value));

    internal override DisunityMeshRenderer ReadSettings()
    {
        var sprite = Read();
        var settings = _other;
        settings.enabled = sprite.enabled;
        settings.sorting_layer = sprite.sorting_layer;
        settings.sorting_order = sprite.sorting_order;
        return settings;
    }

    internal override void WriteSettings(DisunityMeshRenderer value)
    {
        var sprite = Read();
        sprite.enabled = value.enabled;
        sprite.sorting_layer = value.sorting_layer;
        sprite.sorting_order = value.sorting_order;
        Write(sprite);
        _other = value;
    }

    internal override IReadOnlyList<Material?> ReadShared() => [Material.FromNative(Read().lighting)];

    internal override void WriteShared(IReadOnlyList<Material?> materials)
    {
        if (materials.Count != 1) throw new NotSupportedException("a SpriteRenderer has exactly one material");
        ArgumentNullException.ThrowIfNull(materials[0]);
        var sprite = Read();
        sprite.lighting = (uint)materials[0]!.SpriteMaterial;
        Write(sprite);
    }

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        base.CopyManagedState(source, remap);
        _other = ((SpriteRenderer)source)._other;
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
    public bool flipX { get => Read().flip_x == 1; set { var v = Read(); v.flip_x = value ? 1u : 0u; Write(v); } }
    public bool flipY { get => Read().flip_y == 1; set { var v = Read(); v.flip_y = value ? 1u : 0u; Write(v); } }
    public int sortingLayer { get => sortingLayerID; set => sortingLayerID = value; }
    public override unsafe Bounds bounds
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
