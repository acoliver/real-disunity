using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>Materials a player's plans reference: built once from their plan with the shader, queue, keywords, tags,
/// disabled passes and properties, shared by every reference, loadable through Resources, set on renderers, and left
/// null where they did not plan.</summary>
internal static class PlayerMaterialTests
{
    private const string LitShader = "Test/PlayerMaterials/Lit";

    public sealed class Holder : ScriptableObject
    {
        public Material? first = null;
        public Material? second = null;
        public Material? failed = null;
    }

    public sealed class SceneHolder : MonoBehaviour
    {
        public static SceneHolder? Seen;
        public Material? material = null;
        private void Awake() => Seen = this;
    }

    public static void Run()
    {
        PlanHarness.WithPlan(WritePlan, () =>
        {
            ReferencesToOneMaterialGetOneObject();
            AMaterialCarriesItsPlannedValues();
            AMaterialWithoutAShaderGetsTheFallback();
            AMaterialLoadsThroughResources();
            AMaterialThatDidNotPlanFailsNamingTheCause();
            APrefabRendererHoldsThePlannedMaterials();
        });
        ScenePlansBuildTheirMaterials();
    }

    private static JsonArray Floats(params float[] values) => new(values.Select(v => (JsonNode)JsonValue.Create(v)!).ToArray());

    private static JsonArray Pairs(params (string, JsonNode)[] pairs) => new(pairs.Select(p => (JsonNode)new JsonArray(p.Item1, p.Item2)).ToArray());

    private static JsonObject Slot(string name, int? texture) => new()
    {
        ["name"] = name, ["texture"] = texture, ["scale"] = Floats(2, 3), ["offset"] = Floats(0.5f, 0.25f),
    };

    private static JsonObject Lit(int? shader, int? texture) => new()
    {
        ["type"] = "Material", ["name"] = "Lit", ["shader"] = shader, ["render_queue"] = 3100,
        ["keywords"] = new JsonArray("_A", "_B"), ["tags"] = Pairs(("RenderType", "Opaque")), ["disabled_passes"] = new JsonArray("ShadowCaster"),
        ["floats"] = Pairs(("_Cutoff", 0.5f)), ["ints"] = Pairs(("_Mode", 2)), ["colors"] = Pairs(("_Color", Floats(1, 0.5f, 0.25f, 0.75f))),
        ["textures"] = new JsonArray(Slot("_MainTex", texture), Slot("_Bump", null)),
    };

    private static JsonObject Bare() => new()
    {
        ["type"] = "Material", ["name"] = "Bare", ["shader"] = null, ["render_queue"] = -1, ["keywords"] = new JsonArray(), ["tags"] = new JsonArray(),
        ["disabled_passes"] = new JsonArray(), ["floats"] = new JsonArray(), ["ints"] = new JsonArray(), ["colors"] = new JsonArray(), ["textures"] = new JsonArray(),
    };

    private static JsonObject Renderer(params int?[] materials) => new()
    {
        ["type"] = "MeshRenderer", ["enabled"] = true, ["receive_shadows"] = true, ["dynamic_occludee"] = true, ["static_shadow_caster"] = false,
        ["shadow_casting_mode"] = 1, ["motion_vector_mode"] = 1, ["light_probe_usage"] = 1, ["reflection_probe_usage"] = 1,
        ["rendering_layer_mask"] = 1, ["renderer_priority"] = 0, ["sorting_layer_id"] = 0, ["sorting_order"] = 0,
        ["materials"] = new JsonArray(materials.Select(m => (JsonNode?)(m is { } i ? JsonValue.Create(i) : null)).ToArray()),
    };

    private static JsonObject PlannedObject(JsonObject component) => new()
    {
        ["name"] = "Rock", ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = null, ["template"] = true,
        ["position"] = Floats(0, 0, 0), ["rotation"] = Floats(0, 0, 0, 1), ["scale"] = Floats(1, 1, 1), ["rect"] = null,
        ["components"] = new JsonArray(component),
    };

    private static JsonObject ScenePlan(JsonArray assets, JsonArray objects) => new()
    {
        ["scene"] = "rock", ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
        ["differences"] = new JsonArray(), ["assets"] = assets, ["objects"] = objects,
    };

    private static string WritePlan(string directory)
    {
        File.WriteAllBytes(Path.Combine(directory, "albedo.bin"), new byte[8 * 8 * 4]);
        File.WriteAllText(Path.Combine(directory, "holder.json"), """{"first":{"$asset":2},"second":{"$asset":2},"failed":null}""");
        File.WriteAllText(Path.Combine(directory, "rock.json"), ScenePlan([], new JsonArray(PlannedObject(Renderer(2, null, 4)))).ToJsonString());
        var assets = new JsonArray
        {
            new JsonObject { ["type"] = "Shader", ["name"] = LitShader, ["render_queue"] = 3000, ["pass_count"] = 2, ["subshader_count"] = 1 },
            new JsonObject
            {
                ["type"] = "Texture2D", ["name"] = "Albedo", ["width"] = 8, ["height"] = 8, ["format"] = 4, ["mip_count"] = 1, ["readable"] = true,
                ["color_space"] = 1, ["filter_mode"] = 1, ["wrap_u"] = 0, ["wrap_v"] = 0, ["wrap_w"] = 0, ["aniso_level"] = 1, ["mip_bias"] = 0,
                ["data"] = new JsonObject { ["file"] = "albedo.bin", ["offset"] = 0, ["size"] = 8 * 8 * 4 },
            },
            Lit(0, 1),
            new JsonObject { ["type"] = "UnplannedMaterial", ["error"] = "material object 11 of resources.assets: the declarations are truncated" },
            Bare(),
            new JsonObject
            {
                ["type"] = "Script", ["class"] = "PlayerMaterialTests/Holder", ["assembly"] = typeof(PlayerMaterialTests).Assembly.GetName().Name,
                ["name"] = "holder", ["fields"] = "holder.json",
            },
            new JsonObject { ["type"] = "Prefab", ["name"] = "Rock", ["plan"] = "rock.json" },
        };
        string[] paths = ["shaders/lit", "textures/albedo", "materials/lit", "materials/broken", "materials/bare", "config/holder", "prefabs/rock"];
        string[] classes = ["Shader", "Texture2D", "Material", "Material", "Material", "MonoBehaviour", "GameObject"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static void ReferencesToOneMaterialGetOneObject()
    {
        var holder = Resources.Load<Holder>("config/holder")!;
        Expect.That(holder.first is not null && ReferenceEquals(holder.first, holder.second), "two references to one planned material are one object");
        Expect.That(holder.failed is null, "a reference to a material that did not plan binds null");
        Expect.That(ReferenceEquals(Resources.Load<Material>("materials/lit"), holder.first), "Resources.Load<Material> returns the referenced material");
        Console.WriteLine("PASS: references to one planned material are one object and a material that did not plan binds null");
    }

    private static void AMaterialCarriesItsPlannedValues()
    {
        var material = Resources.Load<Material>("materials/lit")!;
        Expect.Equal("Lit", material.name, "the material has its planned name");
        Expect.That(ReferenceEquals(material.shader, Shader.Find(LitShader)), "the material has its planned shader");
        Expect.That(material.rawRenderQueue == 3100 && material.renderQueue == 3100, "the material has its own render queue");
        Expect.Sequence(new[] { "_A", "_B" }, material.shaderKeywords, "the material has its valid keywords");
        Expect.Equal("Opaque", material.GetTag("RenderType", false), "the material has its tags");
        Expect.That(!material.GetShaderPassEnabled("ShadowCaster") && material.GetShaderPassEnabled("ForwardBase"), "only the disabled pass is off");
        Expect.Near(0.5f, material.GetFloat("_Cutoff"), "the material has its float");
        Expect.Equal(2, material.GetInt("_Mode"), "the material has its int");
        Expect.Equal(new Color(1, 0.5f, 0.25f, 0.75f), material.GetColor("_Color"), "the material has its color");
        var albedo = material.GetTexture("_MainTex");
        Expect.That(albedo is Texture2D { name: "Albedo" } && ReferenceEquals(albedo, Resources.Load<Texture2D>("textures/albedo")), "the texture slot holds the planned texture");
        Expect.That(material.GetTexture("_Bump") is null && material.HasTexture("_Bump"), "an empty texture slot is present and null");
        Expect.That(material.GetTextureScale("_MainTex") == new Vector2(2, 3) && material.GetTextureOffset("_MainTex") == new Vector2(0.5f, 0.25f), "the slot has its scale and offset");
        Console.WriteLine("PASS: a planned material carries its name, shader, queue, keywords, tags, passes and properties");
    }

    private static void AMaterialWithoutAShaderGetsTheFallback()
    {
        var bare = Resources.Load<Material>("materials/bare")!;
        Expect.That(bare.shader is not null && bare.shader.name == "Hidden/InternalErrorShader", "a material without a shader gets the fallback shader");
        Expect.That(bare.rawRenderQueue == -1 && bare.renderQueue == bare.shader!.renderQueue, "a queue of -1 is the shader's");
        Console.WriteLine("PASS: a material without a shader gets the fallback shader and the shader's queue");
    }

    private static void AMaterialLoadsThroughResources()
    {
        var lit = Resources.LoadAll<Material>("materials/lit");
        Expect.That(lit.Length == 1 && ReferenceEquals(lit[0], Resources.Load<Material>("materials/lit")), "LoadAll<Material> of a path returns its material");
        Expect.That(Resources.LoadAll<Texture2D>("materials").Length == 0, "a material is not a Texture2D");
        var error = Expect.Catch<InvalidOperationException>(() => Resources.LoadAll<Material>("materials"), "loading a folder that holds a material that did not plan");
        Expect.That(error.Message.Contains("truncated"), $"the folder load names the cause, got {error.Message}");
        Console.WriteLine("PASS: Resources.LoadAll<Material> returns planned materials and fails on one that did not plan");
    }

    private static void AMaterialThatDidNotPlanFailsNamingTheCause()
    {
        var error = Expect.Catch<InvalidOperationException>(() => Resources.Load<Material>("materials/broken"), "loading a material that did not plan");
        Expect.That(error.Message.Contains("truncated") && error.Message.Contains("material object 11"), $"the failure names the cause, got {error.Message}");
        Console.WriteLine("PASS: loading a material that did not plan fails naming the cause");
    }

    private static void APrefabRendererHoldsThePlannedMaterials()
    {
        var rock = Resources.Load<GameObject>("prefabs/rock")!;
        var slots = rock.GetComponent<MeshRenderer>()!.sharedMaterials;
        Expect.That(slots.Length == 3 && ReferenceEquals(slots[0], Resources.Load<Material>("materials/lit")) && slots[1] is null
            && ReferenceEquals(slots[2], Resources.Load<Material>("materials/bare")), "a prefab renderer's slots hold the planned materials and an empty slot");
        Console.WriteLine("PASS: a prefab renderer's slots are the planned materials");
    }

    private static void ScenePlansBuildTheirMaterials()
    {
        var shader = new JsonObject { ["type"] = "Shader", ["name"] = "Test/PlayerMaterials/SceneLit", ["render_queue"] = 2000, ["pass_count"] = 1, ["subshader_count"] = 1 };
        var material = Lit(0, null);
        material["textures"] = new JsonArray(Slot("_MainTex", null));
        var scripted = new JsonObject
        {
            ["type"] = "Script", ["class"] = "PlayerMaterialTests/SceneHolder", ["assembly"] = typeof(PlayerMaterialTests).Assembly.GetName().Name,
            ["enabled"] = true, ["fields"] = new JsonObject { ["material"] = new JsonObject { ["$asset"] = 1 } },
        };
        var holder = PlannedObject(scripted);
        holder["template"] = false;
        holder["components"]!.AsArray().Add(Renderer(1, null));
        var plan = ScenePlan(new JsonArray(shader, material), new JsonArray(holder));
        DisunityHost.LoadWorld();
        try
        {
            var assembly = typeof(PlayerMaterialTests).Assembly;
            using var scene = DisunityHost.LoadScene(plan.ToJsonString(), new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
            var seen = SceneHolder.Seen!;
            var slots = scene.Objects[0].GetComponent<MeshRenderer>()!.sharedMaterials;
            Expect.That(seen.material is not null && slots.Length == 2 && ReferenceEquals(slots[0], seen.material) && slots[1] is null,
                "a scene renderer's slot and a script field meet the same planned material");
            Expect.That(seen.material!.name == "Lit" && seen.material.shader.name == "Test/PlayerMaterials/SceneLit" && seen.material.renderQueue == 3100,
                "the scene material has its name, shader and queue");
            Expect.That(seen.material.HasTexture("_MainTex") && seen.material.GetTexture("_MainTex") is null, "a scene material's texture slot is empty");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: a scene plan builds its materials once for renderer slots and script fields");
    }
}
