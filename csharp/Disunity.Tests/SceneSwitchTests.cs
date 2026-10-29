using System.Collections;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.SceneManagement;

/// <summary>Loading another scene of the build at run time: the build list, load by name, path and index, what a
/// Single load destroys and keeps, the events, and the asynchronous operation.</summary>
internal static class SceneSwitchTests
{
    private static readonly List<string> Log = new();

    private sealed class Probe : MonoBehaviour
    {
        private void Awake() => Log.Add($"Awake:{name}");
        private void OnEnable() => Log.Add($"OnEnable:{name}");
        private void Start() => Log.Add($"Start:{name}");
        private void OnDisable() => Log.Add($"OnDisable:{name}");
        private void OnDestroy() => Log.Add($"OnDestroy:{name}");
    }

    private sealed class Keeper : MonoBehaviour
    {
        private void Awake() => DontDestroyOnLoad(gameObject);
        private void Update() => Log.Add("Update:Keeper");
        public IEnumerator Wait(AsyncOperation operation)
        {
            yield return operation;
            Log.Add($"resumed:{SceneManager.GetActiveScene().name}");
        }
    }

    private static string Script(string type) =>
        "{\"type\":\"Script\",\"class\":\"SceneSwitchTests/" + type + "\",\"assembly\":\"" + typeof(SceneSwitchTests).Assembly.GetName().Name + "\",\"enabled\":true,\"fields\":{}}";

    private static string Object((string Name, string Script) value) =>
        "{\"name\":\"" + value.Name + "\",\"tag\":\"Untagged\",\"layer\":0,\"active\":true,\"parent\":null,\"template\":false,"
        + "\"position\":[0,0,0],\"rotation\":[0,0,0,1],\"scale\":[1,1,1],\"rect\":null,\"components\":[" + Script(value.Script) + "]}";

    private static string Plan(string scene, params (string Name, string Script)[] objects) =>
        "{\"scene\":\"" + scene + "\",\"assets\":[],\"sprites\":[],\"textures\":[],\"fonts\":[],\"axes\":[],\"differences\":[],\"objects\":["
        + string.Join(",", objects.Select(Object)) + "]}";

    private static readonly SceneLoadOptions Options = new(typeof(SceneSwitchTests).Assembly, _ => [])
    {
        Assemblies = _ => typeof(SceneSwitchTests).Assembly,
    };

    private static readonly BuildScene[] Build =
    [
        new("First", "Assets/Scenes/First.unity", () => Plan("level0", ("OldA", "Probe"), ("Keeper", "Keeper"))),
        new("Second", "Assets/Scenes/Second.unity", () => Plan("level1", ("NewB", "Probe"))),
        new("Broken", "Assets/Scenes/Broken.unity", null, "object 3 does not decode"),
    ];

    /// <summary>Loads the build's first scene in a fresh world, runs <paramref name="body"/>, and unloads the world.</summary>
    private static void InWorld(Action body)
    {
        Log.Clear();
        DisunityHost.LoadWorld();
        try
        {
            DisunityHost.SetBuildScenes(Build, Options);
            using var first = DisunityHost.LoadScene(Build[0].ReadPlan!(), Options);
            DisunityHost.Step(0.01f);
            Log.Clear();
            body();
        }
        finally
        {
            DisunityHost.UnloadWorld();
            SceneManager.ClearHandlers();
        }
    }

    public static void Run()
    {
        TheBuildListNamesTheScenes();
        ASingleLoadHappensAtTheNextFrame();
        ASingleLoadKeepsDontDestroyOnLoadRoots();
        EventsComeInOrder();
        ScenesAreFoundByNamePathAndIndex();
        UnknownAndUnsupportedLoadsThrow();
        AsyncLoadReportsProgressAndCompletion();
        AHeldAsyncLoadWaitsForActivation();
        ACoroutineResumesAfterTheLoad();
        AProjectRunHoldsOneScene();
        Console.WriteLine("PASS: loading build scenes at run time by name, path and index, Single load, DontDestroyOnLoad, events and LoadSceneAsync");
    }

    private static void TheBuildListNamesTheScenes() => InWorld(() =>
    {
        Expect.Equal(3, SceneManager.sceneCountInBuildSettings, "sceneCountInBuildSettings");
        var scene = SceneManager.GetActiveScene();
        Expect.Equal("First", scene.name, "Scene.name");
        Expect.Equal("Assets/Scenes/First.unity", scene.path, "Scene.path");
        Expect.Equal(0, scene.buildIndex, "Scene.buildIndex");
        Expect.Equal("Assets/Scenes/Second.unity", SceneUtility.GetScenePathByBuildIndex(1), "GetScenePathByBuildIndex");
        Expect.Equal(1, SceneUtility.GetBuildIndexByScenePath("Assets/Scenes/Second.unity"), "GetBuildIndexByScenePath");
        Expect.Equal(-1, SceneUtility.GetBuildIndexByScenePath("Assets/Scenes/Nope.unity"), "an unlisted path has no index");
        Expect.Equal("", SceneUtility.GetScenePathByBuildIndex(9), "an index outside the build has no path");
    });

    private static void ASingleLoadHappensAtTheNextFrame() => InWorld(() =>
    {
        SceneManager.LoadScene("Second");
        Expect.Equal("First", SceneManager.GetActiveScene().name, "the old scene stays until the next frame");
        Expect.That(GameObject.Find("OldA") != null && GameObject.Find("NewB") == null, "nothing changed yet");
        DisunityHost.Step(0.01f);
        var scene = SceneManager.GetActiveScene();
        Expect.Equal("Second", scene.name, "the new scene is active after the frame");
        Expect.Equal(1, scene.buildIndex, "buildIndex of the new scene");
        DisunityHost.Step(0.01f);
        Expect.Sequence(new[] { "NewB" }, scene.GetRootGameObjects().Select(g => g.name), "the new scene's roots");
        Expect.That(GameObject.Find("OldA") == null, "the old root is gone");
        Expect.That(Log.Contains("OnDisable:OldA") && Log.Contains("OnDestroy:OldA"), "OnDisable and OnDestroy ran for the old root: " + string.Join(",", Log));
        Expect.That(Log.IndexOf("Awake:NewB") >= 0 && Log.IndexOf("OnEnable:NewB") > Log.IndexOf("Awake:NewB") && Log.IndexOf("Start:NewB") > Log.IndexOf("OnEnable:NewB"),
            "Awake, OnEnable and Start ran for the new scene: " + string.Join(",", Log));
        Expect.That(GameObject.Find("NewB")!.scene == scene, "the new object belongs to the new scene");
    });

    private static void ASingleLoadKeepsDontDestroyOnLoadRoots() => InWorld(() =>
    {
        var keeper = GameObject.Find("Keeper")!;
        SceneManager.LoadScene("Second", LoadSceneMode.Single);
        DisunityHost.Step(0.01f);
        DisunityHost.Step(0.01f);
        Expect.That(keeper != null, "the DontDestroyOnLoad root survives");
        Log.Clear();
        DisunityHost.Step(0.01f);
        Expect.That(Log.Contains("Update:Keeper"), "and keeps updating");
        Expect.That(!SceneManager.GetActiveScene().GetRootGameObjects().Contains(keeper), "it is not a root of the new scene");
    });

    private static void EventsComeInOrder() => InWorld(() =>
    {
        var seen = new List<string>();
        SceneManager.sceneUnloaded += scene => seen.Add($"unloaded:{scene.name}");
        SceneManager.sceneLoaded += (scene, mode) => seen.Add($"loaded:{scene.name}:{mode}:{string.Join("+", Log.Where(l => l.StartsWith("Awake")))}");
        SceneManager.activeSceneChanged += (from, to) => seen.Add($"changed:{from.name}>{to.name}");
        SceneManager.LoadScene(1);
        DisunityHost.Step(0.01f);
        Expect.Sequence(new[] { "unloaded:First", "loaded:Second:Single:Awake:NewB", "changed:First>Second" }, seen, "event order");
    });

    private static void ScenesAreFoundByNamePathAndIndex()
    {
        foreach (var key in new[] { "Second", "Assets/Scenes/Second.unity", "Scenes/Second" })
            InWorld(() =>
            {
                SceneManager.LoadScene(key);
                DisunityHost.Step(0.01f);
                Expect.Equal("Second", SceneManager.GetActiveScene().name, $"LoadScene({key})");
            });
        InWorld(() =>
        {
            SceneManager.LoadScene(1);
            DisunityHost.Step(0.01f);
            Expect.Equal("Second", SceneManager.GetActiveScene().name, "LoadScene(1)");
        });
    }

    private static void UnknownAndUnsupportedLoadsThrow() => InWorld(() =>
    {
        var unknown = Expect.Catch<ArgumentException>(() => SceneManager.LoadScene("Nowhere"), "an unknown name");
        Expect.That(unknown.Message.Contains("Nowhere"), unknown.Message);
        Expect.Throws<ArgumentException>(() => SceneManager.LoadScene(7), "an index outside the build");
        Expect.Throws<ArgumentException>(() => SceneManager.LoadSceneAsync("Nowhere", LoadSceneMode.Single), "an unknown async name");
        var broken = Expect.Catch<InvalidOperationException>(() => SceneManager.LoadScene("Broken"), "a scene that did not plan");
        Expect.That(broken.Message.Contains("Broken") && broken.Message.Contains("object 3 does not decode"), broken.Message);
        var additive = Expect.Catch<NotSupportedException>(() => SceneManager.LoadScene("Second", LoadSceneMode.Additive), "Additive");
        Expect.That(additive.Message.Contains("Second") && additive.Message.Contains("Additive"), additive.Message);
        Expect.Throws<NotSupportedException>(() => SceneManager.LoadSceneAsync("Second", LoadSceneMode.Additive), "Additive async");
        Expect.Equal("First", SceneManager.GetActiveScene().name, "a failed request changes nothing");
    });

    private static void AsyncLoadReportsProgressAndCompletion() => InWorld(() =>
    {
        var operation = SceneManager.LoadSceneAsync("Second", LoadSceneMode.Single)!;
        var completed = 0;
        operation.completed += _ => completed++;
        Expect.That(!operation.isDone && operation.progress == 0f, "not done before the frame");
        DisunityHost.Step(0.01f);
        Expect.That(operation.isDone && operation.progress == 1f, "done after the frame");
        Expect.Equal(1, completed, "completed handlers run once");
        Expect.Equal("Second", SceneManager.GetActiveScene().name, "the scene loaded");
    });

    private static void AHeldAsyncLoadWaitsForActivation() => InWorld(() =>
    {
        var operation = SceneManager.LoadSceneAsync("Second", LoadSceneMode.Single)!;
        operation.allowSceneActivation = false;
        DisunityHost.Step(0.01f);
        DisunityHost.Step(0.01f);
        Expect.That(!operation.isDone && operation.progress == 0.9f, $"held at 0.9, was {operation.progress}");
        Expect.Equal("First", SceneManager.GetActiveScene().name, "the scene did not change while held");
        operation.allowSceneActivation = true;
        Expect.Equal("First", SceneManager.GetActiveScene().name, "and not before the next frame");
        DisunityHost.Step(0.01f);
        Expect.That(operation.isDone && SceneManager.GetActiveScene().name == "Second", "released, it loads at the next frame");
    });

    private static void ACoroutineResumesAfterTheLoad() => InWorld(() =>
    {
        var operation = SceneManager.LoadSceneAsync("Second", LoadSceneMode.Single)!;
        GameObject.Find("Keeper")!.GetComponent<Keeper>()!.StartCoroutine(GameObject.Find("Keeper")!.GetComponent<Keeper>()!.Wait(operation));
        Expect.That(!Log.Any(l => l.StartsWith("resumed")), "waiting");
        for (var i = 0; i < 3; i++) DisunityHost.Step(0.01f);
        Expect.Sequence(new[] { "resumed:Second" }, Log.Where(l => l.StartsWith("resumed")), "the coroutine resumes after the load, once");
    });

    private static void AProjectRunHoldsOneScene()
    {
        Log.Clear();
        DisunityHost.LoadWorld();
        try
        {
            using var loaded = DisunityHost.LoadScene(Plan("Main", ("OldA", "Probe")), Options);
            DisunityHost.Step(0.01f);
            Expect.Equal(1, SceneManager.sceneCountInBuildSettings, "a project's build holds its one plan");
            var other = Expect.Catch<ArgumentException>(() => SceneManager.LoadScene("Other"), "another scene in a project");
            Expect.That(other.Message.Contains("Other"), other.Message);
            Expect.Equal("Main", SceneManager.GetActiveScene().name, "the project scene keeps its plan name");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
