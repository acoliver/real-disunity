using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static class NestedScenarioBehavior
{
    internal static void Run(string path, int mode, int depth = 1)
    {
        using var world = new ReadinessWorld(path, nestedSession: true);
        var manifest = world.Manifest([
            new("probe", new GameObject("nested-scenario"), nameof(NestedScenarioProbe), new { mode, depth }),
            new("peer", new GameObject("nested-healthy-peer"), nameof(ReadinessProbe), new { })]);
        BinderBehavior.Check(world.Load(manifest) == 0, "scenario Initialize: " + BridgeBehavior.LastError());
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var probe = session.Resolve("probe");
        int Value(string field) => (int)probe.GetType().GetField(field)!.GetValue(probe)!;
        world.Frame();
        BinderBehavior.Check(Value("after") == 1, "parent returns after synchronous child completion/suspension");
        if (mode == 0)
        {
            BinderBehavior.Check(Value("moves") == Math.Min(depth, 32), "native depth admission executes no rejected child");
            BinderBehavior.Check(Value("disposals") == Math.Min(depth, 32), "completed child Dispose runs before parent return");
            BinderBehavior.Check(Value("rejected") == (depth > 32 ? 1 : 0), "native depth33 rejected exactly once");
            if (depth == 3) BinderBehavior.Check((string)probe.GetType().GetField("trace")!.GetValue(probe)! == "M1M2M3D3D2D1A", "three-depth native LIFO completes before parent returns");
            BinderBehavior.Check(session.CoroutineCount == 1, "only healthy peer suspended, completed nested roots released");
        }
        else if (mode is 10 or 11 or 12)
        {
            BinderBehavior.Check(Value("disposals") == 1, "throwing child receives exactly one Dispose attempt before return");
            BinderBehavior.Check(session.CallbackFailureCount == 1, "child failure retained once");
            using var failures = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.failures"}"""));
            BinderBehavior.Check(failures.RootElement.GetProperty("data").GetProperty("records")[0].GetProperty("message").GetString()!.Contains("nested sentinel"), "original child exception retained");
        }
        else if (mode is 20 or 21 or 22)
        {
            BinderBehavior.Check(Value("currents") == 1 && Value("disposals") == 1, "Current cancellation reads once and disposes before parent return");
            BinderBehavior.Check(session.CallbackFailureCount == 0, "Current cancellation is not an application failure");
        }
        else if (mode == 50)
        {
            BinderBehavior.Check(Value("rejected") == 1 && Value("moves") > 1000 && Value("moves") < 2048, "native shared root claim budget rejects further child starts");
            BinderBehavior.Check(Value("moves") == Value("disposals"), "budget leaves claims for every admitted immediate Dispose");
        }
        world.Frame();
        if (mode == 1) BinderBehavior.Check(Value("rejected") == 2 && Value("sentinelMoves") == 0, "Current/Dispose starts reject before user child execution");
        if (mode == 30) BinderBehavior.Check(session.CallbackFailureCount == 1, "ancestor join rejected, child disposed, parent retained");
        if (mode == 31) BinderBehavior.Check(session.CallbackFailureCount == 1, "self join rejected and routine disposed");
        if (mode == 40) BinderBehavior.Check(Value("disposals") == 2, "stopped child and immediate replacement dispose once each");
        var peer = session.Resolve("peer");
        BinderBehavior.Check((int)peer.GetType().GetField("updates")!.GetValue(peer)! == 2, "healthy peer runs after child and later native roots remain usable");
        world.Cleanup();
        Console.WriteLine($"PASS nested scenario mode={mode} depth={depth}: synchronous child pipeline, peer continuation and native Closed enforcement and measured managed root drain");
    }
}
