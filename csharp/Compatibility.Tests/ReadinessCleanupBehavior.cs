using System.Collections;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class ReadinessCleanupBehavior
{
    internal static void Run(string path)
    {
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
