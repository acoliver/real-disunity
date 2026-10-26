using System.Text.Json;
using UnityEngine;
using UnityEngine.Rendering.Universal;
using UnityEngine.Tilemaps;

namespace Disunity.Hosting;

internal sealed partial class SceneLoader
{
    private object? Component(GameObject owner, JsonElement value, string where) => value.Str("type") switch
    {
        "Camera" => Camera(owner, value),
        "AudioListener" => Toggle(owner.AddComponent<AudioListener>(), value),
        "GUILayer" => Toggle(owner.AddComponent<GUILayer>(), value),
        "FlareLayer" => Toggle(owner.AddComponent<FlareLayer>(), value),
        "SpriteRenderer" => SpriteRenderer(owner, value),
        "Rigidbody2D" => Body(owner, value, where),
        "Collider2D" => Collider(owner, value),
        "Tilemap" => Tilemap(owner, value, where),
        "Canvas" => Canvas(owner, value, where),
        "CanvasScaler" => Scale(owner, value, where),
        "Text" => Text(owner, value, where),
        "GraphicRaycaster" or "EventSystem" or "StandaloneInputModule" => Unbuilt(where),
        "Light2D" => Light(owner, value),
        "UniversalAdditionalCameraData" => CameraData(owner, value, where),
        "Script" => Script(owner, value, where),
        var other => throw new SceneLoadException($"{where}: {other} components are not supported"),
    };

    private static T Toggle<T>(T behaviour, JsonElement value) where T : Behaviour
    {
        behaviour.enabled = value.Bool("enabled");
        return behaviour;
    }

    /// <summary>UI event components route pointer and navigation input to UI elements, which
    /// the engine does not deliver, so they are not built.</summary>
    private object? Unbuilt(string where)
    {
        differences.Add($"{where}: not built; UI elements receive no pointer or navigation events");
        return null;
    }

    private static Camera Camera(GameObject owner, JsonElement value)
    {
        var camera = owner.AddComponent<Camera>();
        camera.clearFlags = value.Variant<CameraClearFlags>("clear");
        camera.backgroundColor = value.Rgba("background");
        camera.orthographic = true;
        camera.orthographicSize = value.Float("orthographic_size");
        camera.nearClipPlane = value.Float("near");
        camera.farClipPlane = value.Float("far");
        camera.depth = value.Float("depth");
        camera.enabled = value.Bool("enabled");
        return camera;
    }

    private static Material Material(string name) => name switch
    {
        "SpritesDefault" => UnityEngine.Material.SpriteUnlit,
        "SpriteLitDefault" => UnityEngine.Material.SpriteLit,
        _ => throw new SceneLoadException($"renderer material {name} is not supported"),
    };

    private SpriteRenderer SpriteRenderer(GameObject owner, JsonElement value)
    {
        var renderer = owner.AddComponent<SpriteRenderer>();
        renderer.sharedMaterial = Material(value.Str("material"));
        renderer.sortingOrder = value.Int("sorting_order");
        if (value.Index("sprite") is { } sprite) renderer.sprite = sprites[sprite];
        renderer.color = value.Rgba("color");
        renderer.flipX = value.Bool("flip_x");
        renderer.flipY = value.Bool("flip_y");
        renderer.enabled = value.Bool("enabled");
        return renderer;
    }

    private Rigidbody2D Body(GameObject owner, JsonElement value, string where)
    {
        var body = owner.AddComponent<Rigidbody2D>();
        body.bodyType = value.Variant<RigidbodyType2D>("body_type");
        body.mass = value.Float("mass");
        body.gravityScale = value.Float("gravity_scale");
        body.constraints = (RigidbodyConstraints2D)value.Int("constraints");
        if (value.Index("material") is { } material) body.sharedMaterial = (PhysicsMaterial2D)assets[material];
        body.simulated = value.Bool("simulated");
        if (value.Str("interpolation") != "None")
            differences.Add($"{where}: {value.Str("interpolation")} is not applied; the body draws at its last physics step");
        return body;
    }

    private Collider2D Collider(GameObject owner, JsonElement value)
    {
        var shape = value.GetProperty("shape");
        Collider2D collider;
        if (shape.ValueKind == JsonValueKind.String) collider = owner.AddComponent<TilemapCollider2D>();
        else if (shape.TryGetProperty("Box", out var box))
        {
            var rectangle = owner.AddComponent<BoxCollider2D>();
            rectangle.size = box.Vec2("size");
            collider = rectangle;
        }
        else
        {
            var circle = owner.AddComponent<CircleCollider2D>();
            circle.radius = shape.GetProperty("Circle").Float("radius");
            collider = circle;
        }
        collider.isTrigger = value.Bool("trigger");
        collider.offset = value.Vec2("offset");
        if (value.Index("material") is { } material) collider.sharedMaterial = (PhysicsMaterial2D)assets[material];
        collider.enabled = value.Bool("enabled");
        return collider;
    }

    private Tilemap Tilemap(GameObject owner, JsonElement value, string where)
    {
        var map = owner.AddComponent<Tilemap>();
        var renderer = value.GetProperty("renderer");
        if (!renderer.Bool("enabled")) throw new SceneLoadException($"{where}: a disabled TilemapRenderer is not supported");
        if (renderer.Str("material") != "SpritesDefault")
            differences.Add($"{where}: tiles draw unlit; {renderer.Str("material")} would light them with the scene's 2D lights");
        DisunityHost.ConfigureTilemap(map, new TilemapSettings(value.Vec2("cell_size"), value.Vec2("cell_gap"),
            value.Vec2("tile_anchor"), 0, renderer.Int("sorting_order")));
        map.color = value.Rgba("color");
        var palette = value.GetProperty("palette").EnumerateArray().ToList();
        for (var i = 0; i < palette.Count; i++) DisunityHost.SetTile(map, i, Tile(palette[i]));
        var cells = value.GetProperty("cells").EnumerateArray()
            .Select(cell => (new Vector2Int(cell[0][0].GetInt32(), cell[0][1].GetInt32()), cell[1].GetInt32()))
            .ToList();
        DisunityHost.SetCells(map, cells);
        map.enabled = value.Bool("enabled");
        return map;
    }

    private TileDefinition Tile(JsonElement tile)
    {
        var index = tile.Index("sprite");
        var collider = tile.Variant<TileColliderType>("collider");
        IReadOnlyList<Vector2[]> shape = [];
        if (collider == TileColliderType.Sprite && index is { } sprite)
            shape = plan.GetProperty("sprites")[sprite].GetProperty("physics_shape").EnumerateArray()
                .Select(path => path.EnumerateArray().Select(p => new Vector2(p[0].GetSingle(), p[1].GetSingle())).ToArray())
                .ToList();
        return new TileDefinition(index is { } i ? sprites[i] : null, tile.Rgba("color"), collider, shape, tile.Vec2("offset"));
    }

    private static Light2D Light(GameObject owner, JsonElement value)
    {
        var light = owner.AddComponent<Light2D>();
        light.lightType = Light2D.LightType.Global;
        light.color = value.Rgba("color");
        light.intensity = value.Float("intensity");
        light.blendStyleIndex = value.Int("blend_style");
        light.affectedSortingLayers = value.GetProperty("target_sorting_layers").EnumerateArray().Select(l => l.GetInt32()).ToArray();
        light.enabled = value.Bool("enabled");
        return light;
    }

    private UniversalAdditionalCameraData CameraData(GameObject owner, JsonElement value, string where)
    {
        var data = owner.AddComponent<UniversalAdditionalCameraData>();
        if (!value.Bool("clear_depth")) throw new SceneLoadException($"{where}: cameras that keep depth are not supported");
        data.renderType = value.Variant<CameraRenderType>("render_type");
        if (value.Int("renderer_index") is var index and not -1) data.SetRenderer(index);
        // The flag enables 3D shadow maps; URP's 2D renderer takes shadows from 2D lights.
        if (value.Bool("render_shadows")) differences.Add($"{where}: renders no 3D shadow maps");
        data.requiresDepthOption = value.Variant<CameraOverrideOption>("depth_texture");
        data.requiresColorOption = value.Variant<CameraOverrideOption>("opaque_texture");
        var post = value.GetProperty("post_processing");
        data.renderPostProcessing = post.Bool("enabled");
        data.antialiasing = post.Str("antialiasing") switch
        {
            "None" => AntialiasingMode.None,
            "Fxaa" => AntialiasingMode.FastApproximateAntialiasing,
            "Smaa" => AntialiasingMode.SubpixelMorphologicalAntiAliasing,
            _ => AntialiasingMode.TemporalAntiAliasing,
        };
        data.stopNaN = post.Bool("stop_nan");
        data.dithering = post.Bool("dithering");
        data.enabled = value.Bool("enabled");
        return data;
    }
}
