using System.Collections;
using CompatibilityBridge;
using UnityEngine;

internal static class LifecycleCleanupMutationBehavior
{
    public static void Run()
    {
        var peer = new GameObject("cleanup-peer");
        var controller = new GameObject("cleanup-controller");
        using var payload = TestGraph.Payload([
            new("peer", peer, nameof(LifecycleCleanupPeer), new { }),
            new("controller", controller, nameof(LifecycleCleanupController), new { peer = TestGraph.Ref("peer") })]);
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        session.Dispatch("Start");
        BinderBehavior.Check(session.CoroutineCount == 1, "Start must enter the real cleanup iterator");
        NativeWorld.Request("object.set", ("object", controller.NativeHandle), ("active", false)).Dispose();
        session.Dispatch("Update");
        BinderBehavior.Check(peer.transform.position.x == 0,
            "coroutine disposal during reconciliation must prevent the disabled peer's next callback");
        BinderBehavior.Check(session.CoroutineCount == 0, "inactive owner's iterator must drain");
        Console.WriteLine("PASS lifecycle cleanup mutation: fresh eligibility after coroutine Dispose changes a peer");
    }
}

public sealed class LifecycleCleanupPeer : MonoBehaviour
{
    private void Update() => transform.position += new Vector3(1, 0, 0);
}

public sealed class LifecycleCleanupController : MonoBehaviour
{
    public MonoBehaviour? peer;
    private IEnumerator Start()
    {
        try { yield return null; }
        finally { peer!.enabled = false; }
    }
}
