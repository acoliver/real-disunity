using System.Collections;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class NestedLifecycleBehavior
{
    internal static void Run(string path, bool moveNext, bool throwing)
    {
        using var world = new ReadinessWorld(path, nestedSession: true);
        var parent = new GameObject("instantiate-parent");
        var root = new GameObject("clone-root");
        var child = new GameObject("clone-child");
        child.transform.SetParent(root.transform, false);
        var manifest = world.Manifest([
            new("parent", parent, nameof(NestedInstantiateProbe), new { prefab = TestGraph.Ref("object-" + root.NativeHandle), moveNext }),
            new("root-script", root, nameof(NestedCloneProbe), new { owner = TestGraph.Ref("parent"), label = "root" }),
            new("child-script", child, nameof(NestedCloneProbe), new { owner = TestGraph.Ref("parent"), label = "child", throwing })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "nested Instantiate load: " + BridgeBehavior.LastError());
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var probe = session.Resolve("parent");
        var before = session.CallbackFailureCount;
        world.Frame();
        var trace = (string)probe.GetType().GetField("trace")!.GetValue(probe)!;
        BinderBehavior.Check(trace == "before,root.awake,child.awake,root.enable,child.enable,root.setup,child.setup,after,",
            "Instantiate drains hierarchy lifecycle before returning: " + trace);
        BinderBehavior.Check(session.CallbackFailureCount == before + (throwing ? 1 : 0), "throwing clone Awake is isolated and recorded exactly once");
        world.Cleanup();
        Console.WriteLine("PASS nested Instantiate synchronous hierarchy Awake/OnEnable, initialized Setup and child failure isolation");
    }
}

public sealed class NestedInstantiateProbe : MonoBehaviour
{
    public GameObject prefab = null!;
    public bool moveNext;
    public string trace = "";
    private IEnumerator? Start() { if (moveNext) return Parent(); Clone(); return null; }
    private IEnumerator Parent() { Clone(); yield return null; }
    private void Clone()
    {
        trace = "before,";
        var clone = UObject.Instantiate(prefab);
        clone.GetComponent<NestedCloneProbe>()!.Setup();
        clone.transform.GetChild(0).GetComponent<NestedCloneProbe>()!.Setup();
        trace += "after,";
    }
}

public sealed class NestedCloneProbe : MonoBehaviour
{
    public NestedInstantiateProbe owner = null!;
    public string label = "";
    public bool throwing;
    private bool _ready;
    private void Awake()
    {
        _ready = true;
        owner.trace += label + ".awake,";
        if (throwing) throw new InvalidOperationException("intentional clone Awake failure");
    }
    private void OnEnable() => owner.trace += label + ".enable,";
    public void Setup()
    {
        if (!_ready) throw new InvalidOperationException("Setup ran before clone Awake");
        owner.trace += label + ".setup,";
    }
}
