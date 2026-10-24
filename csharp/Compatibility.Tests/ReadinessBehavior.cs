using System.Collections;
using UnityEngine;

internal static class ReadinessBehavior
{
    internal static void Run(string path)
    {
        using var world = new ReadinessWorld(path);
        var owner = new GameObject("native-readiness");
        var manifest = world.Manifest([new("probe", owner, nameof(ReadinessProbe), new { })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "explicit bounded readiness profile loads: " + BridgeBehavior.LastError());
        var probe = ((CompatibilityBridge.ScriptSession)NativeIdentity.Runtime!).Resolve("probe");
        int Value(string field) => (int)probe.GetType().GetField(field)!.GetValue(probe)!;
        BinderBehavior.Check(Value("awakes") == 1 && Value("enables") == 1 && Value("starts") == 0, "native Initialize transitions");
        world.Frame();
        BinderBehavior.Check(Value("starts") == 1 && Value("moves") == 1 && Value("updates") == 1, "native Start iterator executes its initial MoveNext once");
        world.Frame();
        BinderBehavior.Check(Value("starts") == 1 && Value("moves") == 2 && Value("disposals") == 1 && Value("updates") == 2, "native subsequent-frame resume and terminal Dispose");
        world.Cleanup();
        BinderBehavior.Check(Value("disables") == 1 && Value("destroys") == 1, "native teardown attempts once");
        Console.WriteLine("PASS readiness Cdecl explicit profile, native lifecycle once flags, initial Start iterator, next-frame readiness and Cleanup");
    }
}
public sealed class ReadinessProbe : MonoBehaviour
{
    public static ReadinessProbe? Instance;
    public int awakes, enables, starts, updates, moves, disposals, disables, destroys;
    private void Awake() { Instance = this; awakes++; }
    private void OnEnable() => enables++;
    private IEnumerator Start()
    {
        starts++;
        try { moves++; yield return null; moves++; }
        finally { disposals++; }
    }
    private void Update() => updates++;
    private void OnDisable() => disables++;
    private void OnDestroy() => destroys++;
}
