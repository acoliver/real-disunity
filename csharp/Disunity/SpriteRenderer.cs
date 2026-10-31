using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>A <see cref="Renderer"/> that draws a <see cref="Sprite"/>. It has exactly one material, any
/// <see cref="Material"/>: a list of any other length throws. The renderer keeps the managed reference and reads it
/// back. The native sprite keeps only a built-in identity: a built-in sprite material, or a copy of one, draws as
/// itself and any other material draws as <see cref="Material.SpriteUnlit"/> until shaders execute (the engine's
/// choice). Its <see cref="Renderer.localBounds"/> is the zero box; <see cref="bounds"/> is the world box of the
/// sprite.</summary>
public sealed class SpriteRenderer : Renderer
{
    private DisunityMeshRenderer _other = Defaults();
    private Material? _material;
    private Vector2 _size;
    private SpriteDrawMode _drawMode;

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

    internal override IReadOnlyList<Material?> ReadShared() => [_material ?? Material.FromNative(Read().lighting)];

    /// <summary>The built-in sprite material the native sprite draws with.</summary>
    internal BuiltinSpriteMaterial NativeMaterial => (BuiltinSpriteMaterial)Read().lighting;

    internal override void WriteShared(IReadOnlyList<Material?> materials)
    {
        if (materials.Count != 1) throw new NotSupportedException("a SpriteRenderer has exactly one material");
        ArgumentNullException.ThrowIfNull(materials[0]);
        var sprite = Read();
        sprite.lighting = (uint)materials[0]!.NativeSpriteIdentity;
        Write(sprite);
        _material = materials[0];
    }

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        base.CopyManagedState(source, remap);
        var sprite = (SpriteRenderer)source;
        _other = sprite._other;
        _material = sprite._material;
        _size = sprite._size;
        _drawMode = sprite._drawMode;
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
    /// <summary>How the sprite is drawn. Stored and read back; nothing draws a sliced or tiled sprite yet, so every mode draws as simple.</summary>
    public SpriteDrawMode drawMode { get => _drawMode; set => _drawMode = value; }

    /// <summary>The size a sliced or tiled sprite is drawn at. Stored and read back; it starts at zero and nothing reads it yet.</summary>
    public Vector2 size { get => _size; set => _size = value; }
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
