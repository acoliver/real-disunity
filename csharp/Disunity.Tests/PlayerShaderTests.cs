using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>Shaders a player's plans reference: built once under their name, shared by every reference, found by
/// <c>Shader.Find</c>, accepted by <c>new Material</c>, loadable through Resources, and left null when they did not
/// plan.</summary>
internal static class PlayerShaderTests
{
    private const string Lit = "Test/PlayerShaders/Lit";

    public sealed class Holder : ScriptableObject
    {
        public Shader? first = null;
        public Shader? second = null;
        public Shader? failed = null;
    }

    public sealed class SceneHolder : MonoBehaviour
    {
        public static SceneHolder? Seen;
        public Shader? shader = null;
        public Shader? same = null;
        public Shader? other = null;
        private void Awake() => Seen = this;
    }

    public static void Run()
    {
        PlanHarness.WithPlan(WritePlan, () =>
        {
            ReferencesToOneShaderGetOneObject();
            AShaderLoadsThroughResourcesAndFeedsMaterial();
            AShaderThatDidNotPlanFailsNamingTheCause();
            ADisagreeingDeclarationKeepsTheFirstAndNotesIt();
        });
        ScenePlansBuildTheirShaders();
    }

    private static JsonObject Declared(string name, int queue, int passes, int subshaders) => new()
    {
        ["type"] = "Shader", ["name"] = name, ["render_queue"] = queue, ["pass_count"] = passes, ["subshader_count"] = subshaders,
    };

    private static string WritePlan(string directory)
    {
        File.WriteAllText(Path.Combine(directory, "holder.json"), """{"first":{"$asset":0},"second":{"$asset":0},"failed":null}""");
        var assets = new JsonArray
        {
            Declared(Lit, 3000, 2, 1),
            new JsonObject { ["type"] = "UnplannedShader", ["error"] = "shader object 3 of resources.assets: the declarations are truncated" },
            new JsonObject
            {
                ["type"] = "Script", ["class"] = "PlayerShaderTests/Holder", ["assembly"] = typeof(PlayerShaderTests).Assembly.GetName().Name,
                ["name"] = "holder", ["fields"] = "holder.json",
            },
            Declared(Lit, 2000, 1, 1),
            Declared("Sprites/Default", 3000, 1, 1),
            Declared(Lit, 3500, 2, 1),
            Declared(Lit, 3000, 3, 1),
            Declared(Lit, 3000, 2, 1),
        };
        string[] paths = ["shaders/lit", "shaders/broken", "config/holder", "shaders/clash", "shaders/sprites", "shaders/queue", "shaders/passes", "shaders/same"];
        string[] classes = ["Shader", "Shader", "MonoBehaviour", "Shader", "Shader", "Shader", "Shader", "Shader"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static void ReferencesToOneShaderGetOneObject()
    {
        var holder = Resources.Load<Holder>("config/holder")!;
        Expect.That(holder.first is not null && ReferenceEquals(holder.first, holder.second), "two references to one planned shader are one object");
        Expect.That(holder.failed is null, "a reference to a shader that did not plan binds null");
        Expect.Equal(Lit, holder.first!.name, "the shader has its declared name");
        Expect.That(holder.first.renderQueue == 3000 && holder.first.passCount == 2 && holder.first.subshaderCount == 1, "the shader has its declared queue and counts");
        Expect.That(ReferenceEquals(Shader.Find(Lit), holder.first) && ReferenceEquals(Shader.Find(Lit.ToUpperInvariant()), holder.first), "Shader.Find finds a player shader by name, ignoring case");
        Console.WriteLine("PASS: references to one planned shader are one object, Shader.Find finds it, and a shader that did not plan binds null");
    }

    private static void AShaderLoadsThroughResourcesAndFeedsMaterial()
    {
        var loaded = Resources.Load<Shader>("shaders/lit");
        Expect.That(ReferenceEquals(loaded, Shader.Find(Lit)), "Resources.Load<Shader> returns the registered shader");
        var material = new Material(loaded!);
        Expect.That(ReferenceEquals(material.shader, loaded), "a material accepts a planned shader");
        var sprites = Resources.Load<Shader>("shaders/sprites");
        Expect.That(ReferenceEquals(sprites, Material.SpriteUnlit.shader), "a planned shader named like a built-in one is the built-in shader");
        Expect.That(Resources.Load<Texture2D>("shaders/lit") is null && Resources.Load<Holder>("shaders/lit") is null, "a shader is not any other type");
        Console.WriteLine("PASS: a planned shader loads through Resources, feeds new Material and joins a built-in of its name");
    }

    private static void AShaderThatDidNotPlanFailsNamingTheCause()
    {
        var error = Expect.Catch<InvalidOperationException>(() => Resources.Load<Shader>("shaders/broken"), "loading a shader that did not plan");
        Expect.That(error.Message.Contains("truncated") && error.Message.Contains("shader object 3"), $"the failure names the cause, got {error.Message}");
        Console.WriteLine("PASS: loading a shader that did not plan fails naming the cause");
    }

    private static void ADisagreeingDeclarationKeepsTheFirstAndNotesIt()
    {
        var first = Shader.Find(Lit)!;
        var clash = Resources.Load<Shader>("shaders/clash")!;
        Expect.That(ReferenceEquals(clash, first) && first.renderQueue == 3000 && first.passCount == 2, "a second declaration of a name returns the first shader with its first values");
        foreach (var path in new[] { "shaders/queue", "shaders/passes", "shaders/same", "shaders/sprites" })
            Expect.That(ReferenceEquals(Resources.Load<Shader>(path), path == "shaders/sprites" ? Material.SpriteUnlit.shader : first), $"{path} is the shader of its name");
        var differences = DisunityHost.Resources!.Differences;
        Expect.That(differences.Count == 3 && differences.All(line => line.Contains("first stays")), $"each disagreeing declaration is noted once and an agreeing one is not, got {string.Join("; ", differences)}");
        Expect.That(differences.Any(line => line.Contains("queue 3500")) && differences.Any(line => line.Contains("3 passes")) && differences.Any(line => line.Contains("queue 2000")), "the notes name the declared values");
        Console.WriteLine("PASS: a disagreeing second declaration keeps the first and adds a difference");
    }

    private static string ScenePlan() => new JsonObject
    {
        ["scene"] = "level0", ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
        ["differences"] = new JsonArray(),
        ["assets"] = new JsonArray { Declared("Test/PlayerShaders/SceneLit", 2200, 3, 2), Declared("Test/PlayerShaders/SceneLit", 3000, 1, 1) },
        ["objects"] = new JsonArray
        {
            new JsonObject
            {
                ["name"] = "A", ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = null, ["template"] = false,
                ["position"] = new JsonArray(0, 0, 0), ["rotation"] = new JsonArray(0, 0, 0, 1), ["scale"] = new JsonArray(1, 1, 1), ["rect"] = null,
                ["components"] = new JsonArray
                {
                    new JsonObject
                    {
                        ["type"] = "Script", ["class"] = "PlayerShaderTests/SceneHolder", ["assembly"] = typeof(PlayerShaderTests).Assembly.GetName().Name,
                        ["enabled"] = true,
                        ["fields"] = new JsonObject
                        {
                            ["shader"] = new JsonObject { ["$asset"] = 0 }, ["same"] = new JsonObject { ["$asset"] = 0 },
                            ["other"] = new JsonObject { ["$asset"] = 1 },
                        },
                    },
                },
            },
        },
    }.ToJsonString();

    private static void ScenePlansBuildTheirShaders()
    {
        DisunityHost.LoadWorld();
        try
        {
            var assembly = typeof(PlayerShaderTests).Assembly;
            var scene = DisunityHost.LoadScene(ScenePlan(), new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
            try
            {
                var seen = SceneHolder.Seen!;
                Expect.That(seen.shader is not null && ReferenceEquals(seen.shader, seen.same), "two scene references to one shader are one object");
                Expect.That(seen.shader!.renderQueue == 2200 && seen.shader.passCount == 3 && seen.shader.subshaderCount == 2, "the scene shader has its declared values");
                Expect.That(ReferenceEquals(seen.other, seen.shader) && ReferenceEquals(Shader.Find("Test/PlayerShaders/SceneLit"), seen.shader), "a second asset of one name is the first shader, and Shader.Find finds it");
                Expect.That(scene.Differences.Count == 1 && scene.Differences[0].Contains("SceneLit"), $"the disagreement is a scene difference, got {string.Join("; ", scene.Differences)}");
            }
            finally { scene.Dispose(); }
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: a scene plan builds its shaders once per name and notes a disagreeing declaration");
    }
}
