using System.Reflection;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.SceneManagement;

/// <summary>The scene manager over the engine's one loaded scene, its events, and the layer names a plan carries.</summary>
internal static class SceneManagerTests
{
    private static readonly List<string> Log = new();

    private sealed class Recorder : MonoBehaviour
    {
        private void Awake() => Log.Add("Awake");
        private void OnEnable() => Log.Add("OnEnable");
        private void Start() => Log.Add("Start");
    }

    private sealed class Captured : ILogHandler, IDisposable
    {
        private readonly ILogHandler _previous = Debug.unityLogger.logHandler;
        public Captured() => Debug.unityLogger.logHandler = this;
        public void LogFormat(LogType logType, UnityEngine.Object? context, string format, params object?[] args) { }
        public void LogException(Exception exception, UnityEngine.Object? context) { }
        public void Dispose() => Debug.unityLogger.logHandler = _previous;
    }

    public static void Run()
    {
        WithoutAScene();
        WithAScene();
        UnloadAndHandlers();
        HandlerFailures();
        UnsupportedLoads();
        LayerNames();
        Console.WriteLine("PASS: SceneManager scenes, loaded and unloaded events, unsupported loads and plan layer names");
    }

    private const string PlanText = """
        {"scene":"NAME",LAYERS"assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"A","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"Script","class":"SceneManagerTests/Recorder","assembly":"ASSEMBLY","enabled":true,"fields":{}}]}]}
        """;

    private static string Plan(string name = "level7", string extra = "") => PlanText
        .Replace("NAME", name).Replace("LAYERS", extra).Replace("ASSEMBLY", typeof(SceneManagerTests).Assembly.GetName().Name);

    private static LoadedScene Load(string plan) =>
        DisunityHost.LoadScene(plan, new SceneLoadOptions(typeof(SceneManagerTests).Assembly, _ => [])
        {
            Assemblies = _ => typeof(SceneManagerTests).Assembly,
        });

    private static void WithoutAScene()
    {
        DisunityHost.LoadWorld();
        try
        {
            Expect.That(!SceneManager.GetActiveScene().IsValid(), "no active scene");
            Expect.Equal(0, SceneManager.sceneCount, "sceneCount");
            Expect.Throws<ArgumentException>(() => SceneManager.GetSceneAt(0), "GetSceneAt(0)");
            Expect.Throws<ArgumentException>(() => SceneManager.SetActiveScene(default), "SetActiveScene of an invalid scene");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static Scene NotLoaded()
    {
        object boxed = default(Scene);
        typeof(Scene).GetProperty("Handle", BindingFlags.Instance | BindingFlags.NonPublic)!.SetValue(boxed, 99);
        return (Scene)boxed;
    }

    private static void WithAScene()
    {
        Log.Clear();
        var seen = new List<string>();
        void OnLoaded(Scene scene, LoadSceneMode mode) => seen.Add($"loaded:{scene.name}:{mode}:{string.Join(",", Log)}");
        SceneManager.sceneLoaded += OnLoaded;
        DisunityHost.LoadWorld();
        try
        {
            Load(Plan());
            Expect.Sequence(new[] { "loaded:level7:Single:Awake,OnEnable" }, seen, "sceneLoaded comes after Awake and OnEnable and before Start");
            DisunityHost.Step(0.01f);
            Expect.Sequence(new[] { "Awake", "OnEnable", "Start" }, Log, "Start follows in the first step");
            var active = SceneManager.GetActiveScene();
            Expect.That(active.IsValid() && active.name == "level7" && active == GameObject.Find("A")!.scene, "the active scene is the loaded one");
            Expect.Equal(1, SceneManager.sceneCount, "sceneCount");
            Expect.That(SceneManager.GetSceneAt(0) == active, "GetSceneAt(0)");
            Expect.Throws<ArgumentException>(() => SceneManager.GetSceneAt(1), "GetSceneAt(1)");
            Expect.Throws<ArgumentException>(() => SceneManager.GetSceneAt(-1), "GetSceneAt(-1)");
            Expect.That(SceneManager.SetActiveScene(active), "SetActiveScene of the loaded scene");
            Expect.That(!SceneManager.SetActiveScene(NotLoaded()), "SetActiveScene of a scene that is not loaded");
            Expect.Throws<ArgumentException>(() => SceneManager.SetActiveScene(default), "SetActiveScene of an invalid scene");
        }
        finally
        {
            DisunityHost.UnloadWorld();
            SceneManager.sceneLoaded -= OnLoaded;
        }
    }

    private static void UnloadAndHandlers()
    {
        var seen = new List<string>();
        SceneManager.sceneLoaded += (scene, mode) => seen.Add("loaded");
        SceneManager.sceneUnloaded += scene => seen.Add($"unloaded:{scene.name}:{scene.IsValid()}");
        DisunityHost.LoadWorld();
        Load(Plan());
        Expect.Sequence(new[] { "loaded" }, seen, "loaded once");
        DisunityHost.UnloadWorld();
        Expect.Sequence(new[] { "loaded", "unloaded:level7:True" }, seen, "sceneUnloaded is raised with the scene while it still exists");
        DisunityHost.LoadWorld();
        try
        {
            Load(Plan("level8"));
            Expect.Sequence(new[] { "loaded", "unloaded:level7:True" }, seen, "handlers are cleared when the world unloads");
        }
        finally { DisunityHost.UnloadWorld(); }
        Expect.Sequence(new[] { "loaded", "unloaded:level7:True" }, seen, "and nothing is raised for a cleared handler");
    }

    private static void HandlerFailures()
    {
        Log.Clear();
        var seen = new List<string>();
        SceneManager.sceneLoaded += (scene, mode) => throw new InvalidOperationException("handler failed");
        SceneManager.sceneLoaded += (scene, mode) => seen.Add("second");
        using var quiet = new Captured();
        DisunityHost.LoadWorld();
        try
        {
            Load(Plan());
            Expect.Sequence(new[] { "second" }, seen, "a later handler still runs");
            Expect.Equal(1, DisunityHost.ScriptExceptionCount, "the failure is reported as a script exception");
            Expect.That(GameObject.Find("A") != null, "the scene stays loaded");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void UnsupportedLoads()
    {
        DisunityHost.LoadWorld();
        try
        {
            var single = Expect.Catch<NotSupportedException>(() => SceneManager.LoadScene("X"), "LoadScene(name)");
            Expect.That(single.Message.Contains("SceneManager.LoadScene(\"X\", Single) is not supported yet: the engine holds one scene"), single.Message);
            var additive = Expect.Catch<NotSupportedException>(() => SceneManager.LoadScene("Y", LoadSceneMode.Additive), "LoadScene(name, mode)");
            Expect.That(additive.Message.Contains("SceneManager.LoadScene(\"Y\", Additive) is not supported yet: the engine holds one scene"), additive.Message);
            var async = Expect.Catch<NotSupportedException>(() => SceneManager.LoadSceneAsync("Z", LoadSceneMode.Additive), "LoadSceneAsync");
            Expect.That(async.Message.Contains("SceneManager.LoadSceneAsync(\"Z\", Additive) is not supported yet"), async.Message);
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static string Layers(params (int Index, string Name)[] named)
    {
        var names = new string[32];
        Array.Fill(names, "");
        foreach (var (index, name) in named) names[index] = name;
        return "\"layers\":[" + string.Join(",", names.Select(name => $"\"{name}\"")) + "],";
    }

    private static void LayerNames()
    {
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.SetUserLayerName(20, "Host");
            Load(Plan(extra: Layers((0, "Default"), (5, "UI"), (7, "Lights"), (12, "Enemies"))));
            Expect.Equal(7, LayerMask.NameToLayer("Lights"), "a project names a layer below 8 that has no built-in name");
            Expect.Equal(12, LayerMask.NameToLayer("Enemies"), "every named user layer");
            Expect.Equal(5, LayerMask.NameToLayer("UI"), "the plan's names for built-in layers");
            Expect.Equal(-1, LayerMask.NameToLayer("Water"), "a built-in name the plan leaves empty is gone");
            Expect.Equal(-1, LayerMask.NameToLayer("Host"), "a plan with layers replaces the host's user layer names");
            Expect.Equal(1 << 7, LayerMask.GetMask("Lights"), "GetMask");
        }
        finally { DisunityHost.UnloadWorld(); }
        DisunityHost.LoadWorld();
        try
        {
            Load(Plan(extra: Layers((10, "Other"))));
            Expect.Equal(-1, LayerMask.NameToLayer("Lights"), "the next plan's layers replace the earlier ones");
            Expect.Equal(10, LayerMask.NameToLayer("Other"), "the new name");
            DisunityHost.UnloadWorld();
            DisunityHost.LoadWorld();
            DisunityHost.SetUserLayerName(21, "Kept");
            Load(Plan());
            Expect.Equal(21, LayerMask.NameToLayer("Kept"), "a plan without layers leaves the host's names");
            DisunityHost.ClearUserLayerNames();
            Expect.Equal(4, LayerMask.NameToLayer("Water"), "clearing restores the built-in names");
            Expect.Equal(-1, LayerMask.NameToLayer("Kept"), "and forgets the others");
        }
        finally
        {
            DisunityHost.ClearUserLayerNames();
            DisunityHost.UnloadWorld();
        }
    }
}
