using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The render pipeline assets a player's resource plan names are built and set before its first scene
/// loads: the default asset, the asset of each quality level and the global settings of the pipeline type; an asset
/// that fails to build is logged by name and left null.</summary>
internal static class PlayerPipelineTests
{
    public sealed class TestPipeline : RenderPipeline { }

    public sealed class HighAsset : RenderPipelineAsset<TestPipeline>
    {
        public int level = -1;
        public ComputeShader? compute = null;
    }

    public sealed class ThrowingAsset : RenderPipelineAsset
    {
        public ThrowingAsset() => throw new InvalidOperationException("constructor failed");
    }

    public sealed class GlobalAsset : RenderPipelineGlobalSettings { }

    public sealed class Recorder : MonoBehaviour
    {
        public static RenderPipelineAsset? SeenAtAwake;
        private void Awake() => SeenAtAwake = GraphicsSettings.defaultRenderPipeline;
    }

    private const string ScenePlan = """{"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[{"name":"A","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[{"type":"Script","class":"PlayerPipelineTests/Recorder","assembly":"ASSEMBLY","enabled":true,"fields":{}}]}]}""";

    private const string Missing = "PlayerPipelineTests/NoSuchAsset";

    public static void Run()
    {
        SetsTheAssetsBeforeTheFirstSceneAwakes();
        AFailingAssetLeavesThePipelineNullAndLogsTheCause();
        APlanWithoutGraphicsChangesNothing();
        QualityLevelAssets();
        AlwaysIncludedShadersAreFoundByName();
        Console.WriteLine("PASS: player pipeline assets are set before the first scene, a failing asset is logged and left null, quality levels hold their own assets");
    }

    private static string AssemblyName => typeof(PlayerPipelineTests).Assembly.GetName().Name!;

    private const string AlwaysIncluded = "PlayerPipelineTests/AlwaysIncluded";

    private static void AlwaysIncludedShadersAreFoundByName()
    {
        var graphics = Graphics(null, null);
        Expect.That(Shader.Find(AlwaysIncluded) == null, "no shader of that name before the plan is applied");
        graphics["always_included_shaders"] = new JsonArray(5, 2);
        With(graphics, [], log =>
        {
            var found = Shader.Find(AlwaysIncluded);
            Expect.That(found != null && found.name == AlwaysIncluded, "Shader.Find returns an always included shader that nothing references");
            Expect.Equal(2450, found!.renderQueue, "with its planned render queue");
            Expect.That(log.Any(line => line.Contains("always included shader Low") && line.Contains("not a Shader")), $"an asset that is not a shader is logged: {string.Join(" | ", log)}");
        });
    }

    /// <summary>Writes a plan whose assets are a High pipeline (0), the global settings (1), a Low pipeline (2) and an
    /// asset of a class that does not exist (3), an asset whose constructor throws (4) and a shader that only the always
    /// included list names (5), with <paramref name="graphics"/> as its graphics section.</summary>
    private static string WritePlan(string directory, JsonNode? graphics)
    {
        var assets = new JsonArray();
        var classes = new[] { ("High", "PlayerPipelineTests/HighAsset", """{"level":"3","compute":null}"""), ("Global", "PlayerPipelineTests/GlobalAsset", "{}"),
            ("Low", "PlayerPipelineTests/HighAsset", """{"level":"1"}"""), ("Broken", Missing, "{}"),
            ("Throws", "PlayerPipelineTests/ThrowingAsset", "{}") };
        for (var i = 0; i < classes.Length; i++)
        {
            File.WriteAllText(Path.Combine(directory, $"{i}.json"), classes[i].Item3);
            assets.Add(new JsonObject { ["type"] = "Script", ["class"] = classes[i].Item2, ["assembly"] = AssemblyName, ["name"] = classes[i].Item1, ["fields"] = $"{i}.json" });
        }
        assets.Add(new JsonObject { ["type"] = "Shader", ["name"] = AlwaysIncluded, ["render_queue"] = 2450, ["pass_count"] = 2, ["subshader_count"] = 1 });
        var plan = new JsonObject { ["entries"] = new JsonArray(), ["assets"] = assets, ["differences"] = new JsonArray() };
        if (graphics is not null) plan["graphics"] = graphics;
        var path = Path.Combine(directory, "resources.json");
        File.WriteAllText(path, plan.ToJsonString());
        return path;
    }

    private static JsonObject Graphics(int? pipeline, int? global, params int?[] levels) => new()
    {
        ["render_pipeline"] = pipeline, ["global_settings"] = global,
        ["quality_pipelines"] = new JsonArray(levels.Select(level => (JsonNode?)JsonValue.Create(level)).ToArray()),
    };

    /// <summary>Loads a world and the plan, runs <paramref name="run"/> with the lines the start logged, and unloads both.</summary>
    private static void With(JsonNode? graphics, string[] levels, Action<List<string>> run)
    {
        var directory = Directory.CreateTempSubdirectory("disunity-pipeline-");
        try
        {
            DisunityHost.LoadResources(WritePlan(directory.FullName, graphics), _ => typeof(PlayerPipelineTests).Assembly);
            DisunityHost.LoadWorld();
            try
            {
                if (levels.Length > 0) DisunityHost.SetQualityLevels(levels, levels.Length - 1);
                var log = new List<string>();
                DisunityHost.ApplyRenderPipelines(log.Add);
                run(log);
            }
            finally
            {
                DisunityHost.SetRenderPipeline(null, null);
                GraphicsSettings.UnregisterRenderPipelineSettings<TestPipeline>();
                DisunityHost.UnloadWorld();
            }
        }
        finally
        {
            DisunityHost.UnloadResources();
            directory.Delete(recursive: true);
        }
    }

    private static void SetsTheAssetsBeforeTheFirstSceneAwakes()
    {
        With(Graphics(0, 1, null, 2, null), ["Low", "Medium", "High"], log =>
        {
            var pipeline = GraphicsSettings.defaultRenderPipeline as HighAsset;
            Expect.That(pipeline is not null, "the default pipeline is the planned asset");
            Expect.Equal("High", pipeline!.name, "the pipeline asset's name");
            Expect.Equal(3, pipeline.level, "the pipeline asset's fields are bound");
            Expect.That(pipeline.compute is null, "a compute shader reference binds null");
            Expect.Equal("render pipeline asset: High (PlayerPipelineTests+HighAsset)", log.First(line => line.StartsWith("render pipeline asset")), "the start logs the asset once");
            Expect.Equal(1, log.Count(line => line.StartsWith("render pipeline asset")), "one line per asset");
            Expect.Equal("Global", GraphicsSettings.GetSettingsForRenderPipeline<TestPipeline>()?.name, "the global settings belong to the asset's pipeline type");
            Expect.That(QualitySettings.renderPipeline is null && QualitySettings.GetRenderPipelineAssetAt(0) is null, "levels without an asset name none");
            Expect.Equal("Low", QualitySettings.GetRenderPipelineAssetAt(1)?.name, "the Medium level's asset");
            Expect.That(GraphicsSettings.currentRenderPipeline == pipeline, "a level without an asset uses the default");
            QualitySettings.SetQualityLevel(1);
            Expect.Equal("Low", GraphicsSettings.currentRenderPipeline?.name, "the current level's asset wins");
            Expect.Equal(2, GraphicsSettings.allConfiguredRenderPipelines.Length, "both assets are configured");
            Recorder.SeenAtAwake = null;
            var plan = ScenePlan.Replace("ASSEMBLY", AssemblyName);
            DisunityHost.LoadScene(plan, new SceneLoadOptions(typeof(PlayerPipelineTests).Assembly, _ => []) { Assemblies = _ => typeof(PlayerPipelineTests).Assembly });
            Expect.That(Recorder.SeenAtAwake == pipeline, "the first scene's Awake sees the pipeline");
        });
    }

    private static void AFailingAssetLeavesThePipelineNullAndLogsTheCause()
    {
        With(Graphics(3, 1, null, 2, null), ["Low", "Medium", "High"], log =>
        {
            Expect.That(GraphicsSettings.defaultRenderPipeline is null, "the failed pipeline is left null");
            var failure = log.FirstOrDefault(line => line.Contains("Broken"));
            Expect.That(failure is not null && failure.StartsWith("the render pipeline asset Broken could not be built:") && failure.Contains("NoSuchAsset"), $"the cause names the asset: {string.Join(" | ", log)}");
            Expect.That(log.Any(line => line.Contains("Global") && line.Contains("not registered")), "global settings without a pipeline type are not registered");
            Expect.Equal("Low", QualitySettings.GetRenderPipelineAssetAt(1)?.name, "the other assets still build");
            Expect.That(GraphicsSettings.GetSettingsForRenderPipeline<TestPipeline>() is null, "no settings are registered");
        });
        With(Graphics(4, null), [], log =>
        {
            Expect.That(GraphicsSettings.defaultRenderPipeline is null, "a constructor failure leaves the pipeline null");
            Expect.That(log.Any(line => line.Contains("Throws") && line.Contains("InvalidOperationException: constructor failed") && !line.Contains("TargetInvocation")), $"the log names the constructor's exception: {string.Join(" | ", log)}");
        });
    }

    private static void APlanWithoutGraphicsChangesNothing()
    {
        With(null, [], log =>
        {
            Expect.That(log.Count == 0 && GraphicsSettings.defaultRenderPipeline is null && !GraphicsSettings.isScriptableRenderPipelineEnabled, "nothing is set or logged");
        });
    }

    private static void QualityLevelAssets()
    {
        DisunityHost.LoadWorld();
        try
        {
            var asset = ScriptableObject.CreateInstance<HighAsset>();
            DisunityHost.SetQualityLevels(["A", "B"], 1);
            Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.GetRenderPipelineAssetAt(2), "a level past the last");
            Expect.Throws<ArgumentOutOfRangeException>(() => QualitySettings.GetRenderPipelineAssetAt(-1), "a negative level");
            Expect.Throws<ArgumentException>(() => QualitySettings.SetPipelines([asset]), "one asset for two levels");
            QualitySettings.renderPipeline = asset;
            Expect.That(QualitySettings.GetRenderPipelineAssetAt(1) == asset && QualitySettings.GetRenderPipelineAssetAt(0) is null, "the setter replaces the current level's asset");
            DisunityHost.SetQualityLevels(["A", "B"], 0);
            Expect.That(QualitySettings.GetRenderPipelineAssetAt(1) is null, "new levels start without assets");
        }
        finally { DisunityHost.UnloadWorld(); }
        DisunityHost.LoadWorld();
        try { Expect.That(QualitySettings.renderPipeline is null && QualitySettings.GetRenderPipelineAssetAt(0) is null, "a loading world clears the level assets"); }
        finally { DisunityHost.UnloadWorld(); }
    }
}
