using Disunity.Hosting;
using UnityEngine;

internal static class ConstructionTests
{
    private sealed class Recorder : MonoBehaviour
    {
        public static readonly List<string> Log = new();
        public string Label = "";
        public Recorder? Peer;
        public int Bound;
        private void Awake()
        {
            var visible = UnityEngine.Object.FindObjectsByType<Recorder>(FindObjectsSortMode.None).Length;
            Log.Add($"awake {Label} peer={Peer?.Label} bound={Bound} visible={visible}");
        }
        private void OnEnable() => Log.Add($"enable {Label}");
        private void Start() => Log.Add($"start {Label}");
    }

    /// <summary>Sizes of the host's managed tables, which must return to their prior sizes.</summary>
    private static (int Objects, int Scripts, int Components) HostTables()
    {
        int Count(string name) => ((System.Collections.ICollection)typeof(DisunityHost)
            .GetField(name, System.Reflection.BindingFlags.NonPublic | System.Reflection.BindingFlags.Static)!
            .GetValue(null)!).Count;
        return (Count("Tracked"), Count("Scripts"), Count("NativeComponents"));
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void RequireLog(params string[] expected)
    {
        Require(Recorder.Log.SequenceEqual(expected),
            $"expected [{string.Join(", ", expected)}], got [{string.Join(", ", Recorder.Log)}]");
        Recorder.Log.Clear();
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            FieldsAndPeersAreBoundBeforeAnyAwake();
            DisposingRemovesEverythingTheConstructionCreated();
            DisposingPreservesUnrelatedPendingDestroy();
            AddingScriptsNeverActivatesTheirObject();
            Console.WriteLine("PASS: batch construction binds before Awake, hides until activation and disposes cleanly");
        }
        finally
        {
            Recorder.Log.Clear();
            DisunityHost.UnloadWorld();
        }
    }

    private static void FieldsAndPeersAreBoundBeforeAnyAwake()
    {
        var construction = DisunityHost.BeginConstruction();
        var a = construction.CreateGameObject().AddComponent<Recorder>();
        var b = construction.CreateGameObject().AddComponent<Recorder>();
        var hidden = construction.CreateGameObject();
        hidden.SetActive(false);
        var waiting = hidden.AddComponent<Recorder>();
        (a.Label, a.Peer, a.Bound) = ("a", b, 7);
        (b.Label, b.Peer, b.Bound) = ("b", a, 7);
        waiting.Label = "waiting";
        RequireLog();
        Require(UnityEngine.Object.FindObjectsByType<Recorder>(FindObjectsSortMode.None).Length == 0,
            "searches must not see constructing objects");
        Require(!a.gameObject.activeInHierarchy, "constructing objects are not yet in the hierarchy");
        construction.Activate();
        RequireLog("awake a peer=b bound=7 visible=2", "awake b peer=a bound=7 visible=2", "enable a", "enable b");
        Require(hidden.activeSelf == false, "activation keeps serialized inactive objects inactive");
        DisunityHost.Step(0.02f);
        RequireLog("start a", "start b");
        hidden.SetActive(true);
        RequireLog("awake waiting peer= bound=0 visible=3", "enable waiting");
        try { construction.Activate(); throw new Exception("activated twice"); }
        catch (InvalidOperationException) { }
        construction.Dispose();
        Require(a.gameObject.activeInHierarchy, "disposing an activated construction keeps its objects");
        foreach (var recorder in new[] { a, b, waiting }) UnityEngine.Object.Destroy(recorder.gameObject);
        DisunityHost.Step(0.02f);
        Recorder.Log.Clear();
    }

    private static void DisposingRemovesEverythingTheConstructionCreated()
    {
        var before = HostTables();
        GameObject created;
        Recorder script;
        using (var construction = DisunityHost.BeginConstruction())
        {
            created = construction.CreateGameObject();
            script = created.AddComponent<Recorder>();
            created.AddComponent<SpriteRenderer>();
            script.Label = "discarded";
        }
        RequireLog();
        Require(HostTables() == before, $"disposing must release managed tables: {before} -> {HostTables()}");
        try { _ = created.activeSelf; throw new Exception("disposed objects must be destroyed"); }
        catch (ObjectDisposedException) { }
        Require(UnityEngine.Object.FindObjectsByType<Recorder>(FindObjectsSortMode.None).Length == 0,
            "disposed scripts must not be found");
        DisunityHost.Step(0.02f);
        RequireLog();
    }

    private static void DisposingPreservesUnrelatedPendingDestroy()
    {
        var pending = DisunityHost.CreateGameObject();
        UnityEngine.Object.Destroy(pending);
        var before = HostTables();
        GameObject created;
        using (var construction = DisunityHost.BeginConstruction())
        {
            created = construction.CreateGameObject();
            created.AddComponent<Recorder>();
            created.AddComponent<SpriteRenderer>();
        }
        RequireLog();
        Require(HostTables() == before, "abort must release only the construction's managed entries");
        Require(pending != null, "unrelated pending Destroy must stay non-null before frame cleanup");
        Require(created == null, "discarded construction object must be null immediately");
        DisunityHost.Step(0.02f);
        Require(pending == null, "unrelated pending Destroy must retire at frame cleanup");
        RequireLog();
    }

    private static void AddingScriptsNeverActivatesTheirObject()
    {
        var inactive = DisunityHost.CreateGameObject();
        inactive.SetActive(false);
        var late = inactive.AddComponent<Recorder>();
        late.Label = "late";
        RequireLog();
        Require(!inactive.activeSelf, "AddComponent must not activate its object");
        inactive.SetActive(true);
        RequireLog("awake late peer= bound=0 visible=1", "enable late");
        var active = DisunityHost.CreateGameObject();
        var immediate = active.AddComponent<Recorder>();
        Require(Recorder.Log.SequenceEqual(new[] { "awake  peer= bound=0 visible=2", "enable " }),
            $"AddComponent on an active object wakes at once, got [{string.Join(", ", Recorder.Log)}]");
        Recorder.Log.Clear();
        UnityEngine.Object.Destroy(inactive);
        UnityEngine.Object.Destroy(immediate.gameObject);
        DisunityHost.Step(0.02f);
        Recorder.Log.Clear();
    }
}
