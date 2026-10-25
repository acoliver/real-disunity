using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class ReadinessAuthorization
{
    internal static void Run(string path, bool nestedSession = false)
    {
        using var world = new ReadinessWorld(path, nestedSession: nestedSession);
        var manifest = world.Manifest([new("probe", new GameObject("authorization"), nameof(ReadinessProbe), new { })]);
        NativeIdentity.Reset();
        RejectDuplicateProfiles(manifest);
        foreach (var profile in new JsonNode?[] { null, JsonValue.Create(1), JsonValue.Create("unknown"), new JsonObject() })
        {
            var bad = manifest.DeepClone();
            bad["requirements"]!["scheduler_profile"] = profile?.DeepClone();
            BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, bad.ToJsonString()) != 0, "malformed scheduler requirement rejected before binding");
            AssertNoRoots();
        }
        var missingPhase = manifest.DeepClone();
        missingPhase["requirements"]!.AsObject().Remove("execution_profile");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, missingPhase.ToJsonString()) != 0, "scheduler profile requires explicit phase profile");
        var context = world.Open("Initialize");
        foreach (var field in new[] { "world", "session", "frame", "fixed_tick", "phase", "sequence", "token" })
        {
            var forged = JsonNode.Parse(context.GetRawText())!;
            forged[field] = forged[field]!.GetValueKind() == JsonValueKind.Number ? JsonValue.Create(0UL) : JsonValue.Create("invalid");
            if (forged[field]!.ToJsonString() == JsonNode.Parse(context.GetRawText())![field]!.ToJsonString()) forged[field] = 999UL;
            manifest["initial_context"] = forged;
            BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, manifest.ToJsonString()) != 0, "forged Initialize " + field + " rejected");
            AssertNoRoots();
        }
        manifest["initial_context"] = JsonNode.Parse(context.GetRawText());
        var wrongThread = Task.Run(() => BridgeBehavior.Call(&ManagedBridge.LoadFixture, manifest.ToJsonString())).GetAwaiter().GetResult();
        BinderBehavior.Check(wrongThread != 0, "wrong-thread Cdecl Initialize rejected");
        AssertNoRoots();
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, manifest.ToJsonString()) == 0, "valid native Initialize enters once: " + BridgeBehavior.LastError());
        world.Finish(context);
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var before = session.CallbackFailureCount;
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, JsonSerializer.Serialize(new { phase = "Update", context })) != 0, "stale Initialize cannot authorize Update");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, """{"phase":"Update"}""") != 0, "missing phase credential rejects");
        BinderBehavior.Check(session.CallbackFailureCount == before, "rejected roots execute no callback");
        world.Control("frame.open", ("dt", new { num = 0, den = 1 })).Dispose();
        var update = world.Open("Update");
        var request = JsonSerializer.Serialize(new { phase = "Update", context = update });
        BinderBehavior.Check(Task.Run(() => BridgeBehavior.Call(&ManagedBridge.DispatchPhase, request)).GetAwaiter().GetResult() != 0, "wrong-thread phase cannot consume callbacks");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, request) == 0, "owner-thread native claim executes");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, request) != 0, "duplicate native phase cannot invoke callbacks twice");
        var probe = session.Resolve("probe");
        BinderBehavior.Check((int)probe.GetType().GetField("updates")!.GetValue(probe)! == 1, "only authorized callback attempt observed");
        world.Finish(update);
        foreach (var phase in new[] { "UpdateCoroutines", "LateUpdate", "EndOfFrame" }) world.Phase(phase);
        world.Control("frame.end", ("frame", 1UL)).Dispose();
        world.Cleanup();
        Console.WriteLine("PASS readiness Cdecl malformed/unknown profiles and missing/forged/stale/duplicate/wrong-thread roots execute no unauthorized callback");
    }
    internal static void MissingProvider(string path)
    {
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest([new("probe", new GameObject("missing-capability"), nameof(ReadinessProbe), new { })]);
        NativeIdentity.Reset();
        DescriptorRejection.Assert(manifest, "native_readiness_primitives_v1", "readiness primitives capability");
        AssertNoRoots();
        Console.WriteLine("PASS readiness missing descriptor capability rejects before binding/Awake");
    }

    private static void RejectDuplicateProfiles(JsonNode manifest)
    {
        var encoded = manifest.ToJsonString();
        foreach (var field in new[] { "scheduler_profile", "execution_profile" })
        {
            var expected = manifest["requirements"]![field]!.ToJsonString();
            var duplicate = encoded.Replace("\"" + field + "\":" + expected,
                "\"" + field + "\":\"unknown\",\"" + field + "\":" + expected, StringComparison.Ordinal);
            BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, duplicate) != 0, "duplicate required profile rejected before binding");
            AssertNoRoots();
        }
    }
    private static void AssertNoRoots()
    {
        using var stats = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.statistics"}"""));
        var data = stats.RootElement.GetProperty("data");
        BinderBehavior.Check(new[] { "scripts", "coroutines", "bindings" }.All(k => data.GetProperty(k).GetUInt64() == 0), "rejected profile/context retains no managed roots");
    }
}
