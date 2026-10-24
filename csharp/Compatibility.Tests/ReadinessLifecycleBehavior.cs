using System.Collections;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static class ReadinessLifecycleBehavior
{
    internal static void Exceptions(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest(Enumerable.Range(0, 5).Select(role => new ScriptSpec("probe" + role,
            new GameObject("exceptions" + role), nameof(ReadinessFailureProbe), new { role })).ToArray());
        BinderBehavior.Check(world.Load(manifest) == 0, "throwing Awake is an application failure");
        var session = (ScriptSession)NativeIdentity.Runtime!;
        world.Frame(); world.Frame();
        BinderBehavior.Check(Value(session.Resolve("probe0"), "updates") == 2, "healthy peers and later phases continue after exceptions");
        BinderBehavior.Check(Value(session.Resolve("probe1"), "awakes") == 1 && Value(session.Resolve("probe2"), "starts") == 1, "throwing Awake and Start consume native once flags");
        BinderBehavior.Check(session.CallbackFailureCount == 5, "Awake, Start, two Updates and Dispose preserve original failures");
        world.Cleanup();
        Console.WriteLine("PASS readiness throwing Awake/Start/Update/Dispose consumes native attempts and preserves healthy peers/later phases");
    }
    internal static void Mutation(string path)
    {
        using var world = new ReadinessWorld(path);
        var controller = new GameObject("controller"); var disposer = new GameObject("disposer"); var victim = new GameObject("victim");
        var manifest = world.Manifest([
            new("controller", controller, nameof(ReadinessMutationProbe), new { target = TestGraph.Ref("disposer"), mode = 1 }),
            new("disposer", disposer, nameof(ReadinessDisposeProbe), new { peer = TestGraph.Ref("victim") }),
            new("victim", victim, nameof(ReadinessMutationProbe), new { mode = 0 })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "mutation fixture load");
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var victimScript = session.Resolve("victim");
        world.Frame(); world.Frame();
        BinderBehavior.Check(Value(victimScript, "starts") == 1 && Value(victimScript, "updates") == 1, "Dispose mutation suppresses later peer Update using a fresh native eligibility choice");
        BinderBehavior.Check(session.CallbackFailureCount == 1, "throwing mutating Dispose isolated");
        world.Frame();
        BinderBehavior.Check(Value(victimScript, "updates") == 1, "disabled peer remains natively ineligible");
        world.Cleanup();
        Console.WriteLine("PASS readiness native destroy/disposal mutation refresh suppresses later peers despite throwing Dispose");
    }
    internal static void Clone(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest([new("clone", new GameObject("clone"), nameof(ReadinessMutationProbe), new { mode = 2 })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "clone fixture load");
        world.Frame(); world.Frame();
        using var snapshot = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.snapshot"}"""));
        var instances = snapshot.RootElement.GetProperty("data").GetProperty("instances");
        BinderBehavior.Check(instances.GetArrayLength() == 2, "actual cloned Script generation registered");
        var starts = instances.EnumerateArray().Select(v => v.GetProperty("fields").GetProperty("starts").GetInt32()).Order().ToArray();
        BinderBehavior.Check(starts.SequenceEqual(new[] { 1, 2 }), "clone gets one native Start independent of copied source fields");
        BinderBehavior.Check(instances.EnumerateArray().All(v => v.GetProperty("awake").ValueKind == JsonValueKind.Null), "new profile exposes no competing managed lifecycle flags");
        world.Cleanup();
        Console.WriteLine("PASS readiness clones register real generations, Start once independent of copied fields, native lifecycle authority and drained unload");
    }
    private static int Value(UnityEngine.Object instance, string field) => (int)instance.GetType().GetField(field)!.GetValue(instance)!;
}
public sealed class ReadinessFailureProbe : MonoBehaviour
{
    public int role, awakes, starts, updates;
    private void Awake() { awakes++; if (role == 1) throw new ArgumentException("readiness Awake sentinel"); }
    private IEnumerator Start()
    {
        starts++;
        if (role == 2) throw new ArgumentException("readiness Start sentinel");
        try { yield return null; }
        finally { if (role == 4) throw new ArgumentException("readiness Dispose sentinel"); }
    }
    private void Update() { updates++; if (role == 3) throw new ArgumentException("readiness Update sentinel"); }
}
public sealed class ReadinessMutationProbe : MonoBehaviour
{
    public int mode, starts, updates;
    public MonoBehaviour? target;
    private void Start() => starts++;
    private void Update()
    {
        updates++;
        if (mode == 1 && updates == 2 && target is not null) { UnityEngine.Object.Destroy(target.gameObject); target = null; }
        if (mode == 2) { mode = 0; UnityEngine.Object.Instantiate(this); }
    }
}
public sealed class ReadinessDisposeProbe : MonoBehaviour
{
    public MonoBehaviour peer = null!;
    private IEnumerator Start()
    {
        try { yield return null; }
        finally { peer.enabled = false; throw new ArgumentException("readiness mutating Dispose sentinel"); }
    }
}
