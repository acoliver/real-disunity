using System.Collections;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class ReadinessCleanupBehavior
{
    internal static void Run(string path)
    {
        SignatureValidation();
        Mutation(path);
        using var world = new ReadinessWorld(path);
        var owners = new[] { new GameObject("cleanup0"), new GameObject("cleanup1") };
        var manifest = world.Manifest(owners.Select((owner, role) => new ScriptSpec("cleanup" + role,
            owner, nameof(ReadinessCleanupProbe), new { role })).ToArray());
        BinderBehavior.Check(world.Load(manifest) == 0, "Cleanup fixture load");
        world.Frame();
        BinderBehavior.Check(((ScriptSession)NativeIdentity.Runtime!).CoroutineCount == 2, "Cleanup starts from actual suspended roots");
        world.Cleanup();
        using var snapshot = NativeWorld.Request("snapshot");
        foreach (var owner in NativeWorld.Data(snapshot).GetProperty("objects").EnumerateArray())
        {
            var position = owner.GetProperty("transform").GetProperty("position");
            BinderBehavior.Check(position.EnumerateArray().All(v => v.GetSingle() == 1), "native Cleanup authorizes every peer disable/disposal/destroy effect despite failures");
        }
        using var failures = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.failures"}"""));
        BinderBehavior.Check(failures.RootElement.GetProperty("data").GetProperty("total").GetInt32() == 3, "Cleanup preserves original disable/Dispose/destroy exceptions");
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        BinderBehavior.Check(unload() == 0 && unload() == 0, "repeated unload after native Cleanup executes no teardown twice");
        Console.WriteLine("PASS readiness Cleanup drains suspended iterators and all teardown peers under native permission despite OnDisable/Dispose/OnDestroy failures");
    }
    private static void Mutation(string path)
    {
        using var world = new ReadinessWorld(path);
        var owner = new GameObject("cleanup-mutation");
        var manifest = world.Manifest([
            new("first", owner, nameof(ReadinessCleanupMutationProbe), new { peer = TestGraph.Ref("peer") }),
            new("peer", owner, nameof(ReadinessCleanupMutationProbe), new { })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "Cleanup mutation fixture load");
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var first = session.Resolve("first");
        var peer = session.Resolve("peer");
        var context = world.Open("Cleanup");
        var forged = System.Text.Json.Nodes.JsonNode.Parse(context.GetRawText())!;
        forged["token"] = "forged";
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase,
            JsonSerializer.Serialize(new { phase = "Cleanup", context = forged })) != 0, "wrong Cleanup context rejects before callbacks");
        BinderBehavior.Check(Value(first, "disables") == 0 && Value(peer, "disables") == 0, "wrong context enters no Cleanup callback");
        var request = JsonSerializer.Serialize(new { phase = "Cleanup", context });
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, request) == 0, "Cleanup mutation drains peers");
        NativeWorld.AttachNativeLibrary(path, world.World);
        world.Finish(context);
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, request) != 0, "completed Cleanup cannot replay");
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        BinderBehavior.Check(unload() == 0 && unload() == 0, "repeated unload after mutation");
        foreach (var instance in new[] { first, peer })
            BinderBehavior.Check(Value(instance, "disables") == 1 && Value(instance, "destroys") == 1,
                "removed peer and failed mutator receive each Cleanup callback exactly once");
        BinderBehavior.Check(Value(peer, "observed") == 7, "peer observes mutation made before the first callback failed");
        Console.WriteLine("PASS readiness Cleanup mutation-before-peer, wrong context and once-only callback attempts");
    }
    private static int Value(UnityEngine.Object instance, string field) => (int)instance.GetType().GetField(field)!.GetValue(instance)!;
    private static void SignatureValidation()
    {
        BinderBehavior.Check(ScriptMessages.FindCallback(typeof(ReadinessCleanupMutationProbe), "OnDestroy") is not null,
            "closed void Cleanup method is available");
        BinderBehavior.Check(ScriptMessages.FindCallback(typeof(ReadinessCleanupMutationProbe), "Awake") is null,
            "absent Cleanup method remains absent");
        foreach (var type in new[] { typeof(GenericCleanup), typeof(ReturningCleanup) })
        {
            try { ScriptMessages.FindCallback(type, "OnDestroy"); }
            catch (InvalidDataException) { continue; }
            throw new Exception("unsupported Cleanup signature accepted: " + type.FullName);
        }
    }
    private sealed class GenericCleanup : MonoBehaviour { private void OnDestroy<T>() { } }
    private sealed class ReturningCleanup : MonoBehaviour { private int OnDestroy() => 1; }

}
public sealed class ReadinessCleanupProbe : MonoBehaviour
{
    public int role;
    private IEnumerator Start()
    {
        try { yield return new WaitForSeconds(100); }
        finally { var p = transform.position; p.y++; transform.position = p; Fail("Dispose"); }
    }
    private void OnDisable() { var p = transform.position; p.x++; transform.position = p; Fail("OnDisable"); }
    private void OnDestroy() { var p = transform.position; p.z++; transform.position = p; Fail("OnDestroy"); }
    private void Fail(string method) { if (role == 1) throw new ArgumentException("readiness Cleanup " + method + " sentinel"); }
}

public sealed class ReadinessCleanupMutationProbe : MonoBehaviour
{
    public MonoBehaviour? peer;
    public int marker, observed, disables, destroys;
    private void OnDisable()
    {
        disables++;
        if (peer is null) { observed = marker; return; }
        peer.GetType().GetField("marker")!.SetValue(peer, 7);
        peer.enabled = false;
        UnityEngine.Object.Destroy(peer);
        throw new ArgumentException("Cleanup mutation sentinel");
    }
    private void OnDestroy() => destroys++;
}
