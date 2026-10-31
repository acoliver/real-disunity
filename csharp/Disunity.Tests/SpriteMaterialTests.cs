using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>A SpriteRenderer takes any material: it keeps the managed reference and draws as the built-in unlit sprite
/// material unless the material is a built-in sprite material or a copy of one.</summary>
internal static class SpriteMaterialTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            KeepsAGameMaterial();
            Console.WriteLine("PASS: a sprite renderer keeps a game material as its shared material and draws as the unlit sprite material");
            InstancesAGameMaterial();
            Console.WriteLine("PASS: a sprite renderer's material makes one named copy of a game material and keeps it");
            BuiltInMaterialsStillMap();
            Console.WriteLine("PASS: the built-in sprite materials and their copies keep their native identity");
            CloneKeepsTheMaterial();
            Console.WriteLine("PASS: a cloned sprite renderer keeps a game material and a list of any other length still throws");
        }
        finally { DisunityHost.UnloadWorld(); }
        PlannedGameMaterial();
        Console.WriteLine("PASS: a planned sprite renderer binds a planned game material and still takes the built-in names");
    }

    private static SpriteRenderer NewRenderer() => DisunityHost.CreateGameObject().AddComponent<SpriteRenderer>();

    private static Material GameMaterial(string shader = "Test/SpriteMaterial/Game") => new(DisunityHost.RegisterShader(shader));

    private static void KeepsAGameMaterial()
    {
        var renderer = NewRenderer();
        var game = GameMaterial();
        renderer.sharedMaterial = game;
        Expect.That(ReferenceEquals(renderer.sharedMaterial, game), "the shared material is the game material");
        Expect.That(renderer.sharedMaterials is [var only] && ReferenceEquals(only, game), "the list holds it alone");
        Expect.Equal(BuiltinSpriteMaterial.Unlit, renderer.NativeMaterial, "a game material draws as the unlit sprite material");
        renderer.sharedMaterials = [game];
        Expect.That(ReferenceEquals(renderer.sharedMaterial, game), "a one-element list keeps it too");
        Expect.Throws<NotSupportedException>(() => _ = game.SpriteMaterial, "SpriteMaterial of a game material");
    }

    private static void InstancesAGameMaterial()
    {
        var renderer = NewRenderer();
        var game = GameMaterial();
        game.SetFloat("_Cutoff", 0.25f);
        renderer.sharedMaterial = game;
        var copy = renderer.material!;
        Expect.That(copy != game && copy.name.EndsWith("(Instance)") && ReferenceEquals(copy.shader, game.shader), "the copy is named and has the shader");
        Expect.Near(0.25f, copy.GetFloat("_Cutoff"), "the copy has the properties");
        Expect.That(ReferenceEquals(renderer.material, copy) && ReferenceEquals(renderer.sharedMaterial, copy), "the copy is kept and shared");
        Expect.Equal(BuiltinSpriteMaterial.Unlit, renderer.NativeMaterial, "the copy of a game material draws as unlit");
        var other = GameMaterial("Test/SpriteMaterial/Other");
        renderer.material = other;
        Expect.That(ReferenceEquals(renderer.sharedMaterial, other), "setting material keeps the one assigned");
        renderer.materials = [game];
        Expect.That(ReferenceEquals(renderer.material, game), "materials keeps the list assigned");
    }

    private static void BuiltInMaterialsStillMap()
    {
        var renderer = NewRenderer();
        Expect.That(ReferenceEquals(renderer.sharedMaterial, Material.SpriteUnlit), "a new renderer has the unlit sprite material");
        renderer.sharedMaterial = Material.SpriteLit;
        Expect.That(ReferenceEquals(renderer.sharedMaterial, Material.SpriteLit), "the lit material reads back");
        Expect.Equal(BuiltinSpriteMaterial.Lit, renderer.NativeMaterial, "lit draws as lit");
        renderer.sharedMaterial = GameMaterial();
        renderer.sharedMaterial = Material.SpriteLit;
        Expect.Equal(BuiltinSpriteMaterial.Lit, renderer.NativeMaterial, "going back to lit after a game material");
        renderer.sharedMaterial = new Material(Material.SpriteLit);
        Expect.Equal(BuiltinSpriteMaterial.Lit, renderer.NativeMaterial, "a copy of the lit material keeps the lit identity");
        renderer.sharedMaterial = Material.SpriteUnlit;
        Expect.Equal(BuiltinSpriteMaterial.Unlit, renderer.NativeMaterial, "unlit draws as unlit");
        Expect.Throws<ArgumentNullException>(() => renderer.sharedMaterial = null, "a null material");
    }

    private static void CloneKeepsTheMaterial()
    {
        var renderer = NewRenderer();
        var game = GameMaterial();
        renderer.sharedMaterial = game;
        var clone = UnityEngine.Object.Instantiate(renderer);
        Expect.That(ReferenceEquals(clone.sharedMaterial, game), "the clone has the game material");
        Expect.Throws<NotSupportedException>(() => renderer.sharedMaterials = [], "no material on a sprite");
        Expect.Throws<NotSupportedException>(() => renderer.sharedMaterials = [game, game], "two materials on a sprite");
    }

    private static JsonObject Plan(JsonArray assets, string materialJson) => new()
    {
        ["scene"] = "level0", ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(),
        ["axes"] = new JsonArray(), ["differences"] = new JsonArray(), ["assets"] = assets,
        ["objects"] = new JsonArray(Planned("Game", materialJson), Planned("Lit", "\"SpriteLitDefault\"")),
    };

    private static JsonObject Planned(string name, string materialJson)
    {
        var sprite = JsonNode.Parse($$"""{"type":"SpriteRenderer","enabled":true,"sorting_order":2,"material":{{materialJson}},"sprite":null,"color":[1,1,1,1],"flip_x":false,"flip_y":false}""")!;
        return new JsonObject
        {
            ["name"] = name, ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = null, ["template"] = false,
            ["position"] = new JsonArray(0, 0, 0), ["rotation"] = new JsonArray(0, 0, 0, 1), ["scale"] = new JsonArray(1, 1, 1),
            ["rect"] = null, ["components"] = new JsonArray(sprite),
        };
    }

    private static JsonObject PlannedShader() => new() { ["type"] = "Shader", ["name"] = "Test/SpriteMaterial/Planned", ["render_queue"] = 3000, ["pass_count"] = 1, ["subshader_count"] = 1 };

    private static JsonObject PlannedMaterial() => new()
    {
        ["type"] = "Material", ["name"] = "Glow", ["shader"] = 0, ["render_queue"] = -1, ["keywords"] = new JsonArray(), ["tags"] = new JsonArray(),
        ["disabled_passes"] = new JsonArray(), ["floats"] = new JsonArray(), ["ints"] = new JsonArray(), ["colors"] = new JsonArray(), ["textures"] = new JsonArray(),
    };

    private static void PlannedGameMaterial()
    {
        var plan = Plan(new JsonArray(PlannedShader(), PlannedMaterial()), "1");
        DisunityHost.LoadWorld();
        try
        {
            var assembly = typeof(SpriteMaterialTests).Assembly;
            using var scene = DisunityHost.LoadScene(plan.ToJsonString(), new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
            var game = scene.Objects[0].GetComponent<SpriteRenderer>()!;
            Expect.That(game.sharedMaterial is { name: "Glow" } && game.sharedMaterial.shader.name == "Test/SpriteMaterial/Planned", "the planned game material is bound");
            Expect.Equal(BuiltinSpriteMaterial.Unlit, game.NativeMaterial, "and draws as unlit");
            Expect.Equal(2, game.sortingOrder, "the rest of the sprite renderer loads");
            Expect.That(ReferenceEquals(scene.Objects[1].GetComponent<SpriteRenderer>()!.sharedMaterial, Material.SpriteLit), "a built-in name still maps");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
