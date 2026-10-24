using System.Collections;
using CompatibilityBridge;
using UnityEngine;

internal static class NestedStartBehavior
{
    internal static void Run(string path, bool moveNext = false)
    {
        using var world = new ReadinessWorld(path, nestedSession: true);
        var manifest = world.Manifest([
            new("parent", new GameObject("nested-parent"), nameof(NestedStartProbe), new { moveNext }),
            new("peer", new GameObject("nested-peer"), nameof(NestedStartPeer), new { })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "nested fixture load: " + BridgeBehavior.LastError());
        var session = (ScriptSession)NativeIdentity.Runtime!;
        world.Frame();
        var parent = session.Resolve("parent");
        var trace = (string)parent.GetType().GetField("trace")!.GetValue(parent)!;
        BinderBehavior.Check(trace == (moveNext ? "peer,before,child.move,child.current,after," : "before,child.move,child.current,after,peer,"), "entered lifecycle immediate child ordering: " + trace);
        BinderBehavior.Check(session.CallbackFailureCount == 0, "nested starts must not record application failures");
        world.Frame();
        trace = (string)parent.GetType().GetField("trace")!.GetValue(parent)!;
        BinderBehavior.Check(trace.Contains("child.resume,child.dispose,"), "native future child resume and once Dispose: " + trace);
        world.Cleanup();
        Console.WriteLine("PASS nested entered lifecycle synchronous MoveNext/Current and native future resume/disposal");
    }
}
public sealed class NestedStartProbe : MonoBehaviour
{
    public static NestedStartProbe Instance = null!;
    public string trace = "";
    public bool moveNext;
    private void Awake() => Instance = this;
    private IEnumerator? Start() { if (moveNext) return Parent(); Begin(); return null; }
    private IEnumerator Parent() { Begin(); yield return null; }
    private void Begin() { trace += "before,"; StartCoroutine(new Child(this)); trace += "after,"; }
    private sealed class Child(NestedStartProbe owner) : IEnumerator, IDisposable
    {
        private int moves;
        public bool MoveNext() { owner.trace += moves++ == 0 ? "child.move," : "child.resume,"; return moves == 1; }
        public object? Current { get { owner.trace += "child.current,"; return null; } }
        public void Dispose() => owner.trace += "child.dispose,";
        public void Reset() => throw new NotSupportedException();
    }
}
public sealed class NestedStartPeer : MonoBehaviour
{
    private void Start() => NestedStartProbe.Instance.trace += "peer,";
}
