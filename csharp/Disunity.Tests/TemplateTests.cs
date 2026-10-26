using Disunity.Hosting;
using UnityEngine;

/// A construction that is never activated holds prefab templates, as a scene load does for the
/// prefabs its scripts reference.
internal static class TemplateTests
{
    private static readonly List<string> Log = new();

    private sealed class Brick : MonoBehaviour
    {
        public Sparkle? Sparkle;
        public int Hits;
        private void Awake() => Log.Add($"awake {gameObject.name} hits={Hits} own-sparkle={Sparkle?.transform.parent == transform}");
        private void Start() => Log.Add($"start {gameObject.name}");
    }

    private sealed class Sparkle : MonoBehaviour
    {
        private void Awake() => Log.Add($"awake {gameObject.name}");
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var templates = DisunityHost.BeginConstruction();
            var brick = Prefab(templates);
            for (var i = 0; i < 3; i++) DisunityHost.Step(0.02f);
            RequireLog();
            Require(UnityEngine.Object.FindObjectsByType<Brick>(FindObjectsSortMode.None).Length == 0, "searches skip templates");
            var first = UnityEngine.Object.Instantiate(brick);
            var second = UnityEngine.Object.Instantiate(brick.gameObject).GetComponent<Brick>()!;
            RequireLog("awake Brick(Clone) hits=3 own-sparkle=True", "awake Sparkle",
                "awake Brick(Clone) hits=3 own-sparkle=True", "awake Sparkle");
            Require(!ReferenceEquals(first, brick) && !ReferenceEquals(first.Sparkle, brick.Sparkle), "each clone is independent");
            Require(ReferenceEquals(brick.Sparkle!.transform.parent, brick.transform), "the template keeps its own references");
            Require(UnityEngine.Object.FindObjectsByType<Brick>(FindObjectsSortMode.None).Length == 2, "searches see both clones");
            DisunityHost.Step(0.02f);
            RequireLog("start Brick(Clone)", "start Brick(Clone)");
            templates.Dispose();
            Require(second.gameObject.activeInHierarchy, "clones outlive their template");
            Require(brick == null, "a disposed template is destroyed");
            Console.WriteLine("PASS: never-activated constructions hold prefab templates for Instantiate");
        }
        finally
        {
            Log.Clear();
            DisunityHost.UnloadWorld();
        }
    }

    private static Brick Prefab(Construction templates)
    {
        var root = templates.CreateGameObject();
        root.name = "Brick";
        var child = templates.CreateGameObject();
        child.name = "Sparkle";
        child.transform.SetParent(root.transform, false);
        var brick = root.AddComponent<Brick>();
        (brick.Sparkle, brick.Hits) = (child.AddComponent<Sparkle>(), 3);
        return brick;
    }

    private static void RequireLog(params string[] expected)
    {
        Require(Log.SequenceEqual(expected), $"expected [{string.Join(", ", expected)}], got [{string.Join(", ", Log)}]");
        Log.Clear();
    }

    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
