using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class NestedSelectionBehavior
{
    internal static void Unsupported(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest([new("probe", new GameObject("unsupported-nested"), nameof(NestedStartProbe), new { })]);
        manifest["requirements"]!["scheduler_profile"] = "native_readiness_nested_start_v1";
        NativeIdentity.Reset();
        var before = ReadStatus();
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, manifest.ToJsonString()) != 0, "old provider rejects nested profile before binding");
        BinderBehavior.Check(ReadStatus() == before, "old provider profile rejection consumes no native Initialize");
        Empty();
        Console.WriteLine("PASS nested old provider rejected before Initialize or child side effects");
    }
    internal static void Matching(string path)
    {
        using var world = new ReadinessWorld(path, nestedSession: true);
        var manifest = world.Manifest([new("probe", new GameObject("matched-nested"), nameof(ReadinessProbe), new { })]);
        NativeIdentity.Reset();
        var before = ReadStatus();
        foreach (var profile in new[] { "native_readiness_single_v1", "unknown" })
        {
            var bad = manifest.DeepClone(); bad["requirements"]!["scheduler_profile"] = profile;
            BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, bad.ToJsonString()) != 0, "mismatched scheduler rejects before Initialize");
            BinderBehavior.Check(ReadStatus() == before, "mismatch consumes no native Initialize"); Empty();
        }
        var absent = manifest.DeepClone(); absent["requirements"]!.AsObject().Remove("scheduler_profile");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, absent.ToJsonString()) != 0, "nested native selection requires explicit scheduler manifest");
        BinderBehavior.Check(ReadStatus() == before, "missing scheduler consumes no native Initialize"); Empty();
        BinderBehavior.Check(world.Load(manifest) == 0, "matched scheduler initializes");
        world.Cleanup();
        Console.WriteLine("PASS nested explicit manifest/session matching rejects before binding and Initialize");
    }
    private static string ReadStatus() { using var status = NativeWorld.Request("session.status"); return NativeWorld.Data(status).GetRawText(); }
    private static void Empty()
    {
        using var stats = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.statistics"}"""));
        BinderBehavior.Check(new[] { "scripts", "coroutines", "bindings" }.All(k => stats.RootElement.GetProperty("data").GetProperty(k).GetInt32() == 0), "rejection retains no managed roots");
    }
}
