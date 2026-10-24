using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class ContextualCleanupBehavior
{
    internal static void Run(string path, bool mutation)
    {
        using var world = new ContextualCleanupWorld(path);
        var owner = new GameObject("contextual-cleanup");
        var scripts = Enumerable.Range(0, 2048).Select(role => new ScriptSpec("probe" + role, owner,
            nameof(ContextualCleanupProbe), Fields(role, mutation))).ToArray();
        var manifest = world.Manifest(scripts);
        SelectionControls(manifest);
        world.Load(manifest);
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var type = session.ResolveType(nameof(ContextualCleanupProbe));
        Check(Counter(type, "Initialized") == 4096, "4096 actual serial Initialize CLR method entries");
        var instances = Enumerable.Range(0, 2048).Select(i => session.Resolve("probe" + i)).ToArray();
        var context = world.Open("Cleanup");
        var envelope = world.Reserve(context);
        var raw = envelope.GetRawText();
        Check(envelope.GetProperty("callbacks").GetArrayLength() == 4096, "native reserved 4096 records");
        MalformedControls(world, context, envelope, type);
        var calls = world.DispatchCalls;
        Check(world.Dispatch(raw) == 0, "actual contextual dispatch: " + BridgeBehavior.LastError());
        Check(world.DispatchCalls - calls == 1, "exactly one Cdecl DispatchPhase invocation for successful 4096-record batch");
        Check(Counter(type, "Entries") == 4096 && Counter(type, "Throws") == 43, "4096 actual CLR Cleanup entries, 43 thrown callback exceptions");
        foreach (var instance in instances)
            Check((int)type.GetField("disables")!.GetValue(instance)! == 1 && (int)type.GetField("destroys")!.GetValue(instance)! == 1,
                "each real managed component receives each method exactly once");
        Check(Counter(type, "Mutations") == (mutation ? 1 : 0), "sibling mutation executes once");
        using var next = NativeWorld.Request("callbacks.next", ("context", context));
        Check(NativeWorld.Data(next).ValueKind == JsonValueKind.Null, "all native ticket obligations drained");
        using var failures = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.failures"}"""));
        Check(failures.RootElement.GetProperty("data").GetProperty("total").GetInt64() == 43, "all 43 original failures retained");
        Check(world.Dispatch(raw) != 0 && Counter(type, "Entries") == 4096, "completed batch replay executes no callback body");
        world.Finish(context);
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        Check(unload() == 0, "Closed-only contextual unload: " + BridgeBehavior.LastError());
        Check(unload() == 0, "completed contextual unload is idempotent: " + BridgeBehavior.LastError());
        Check(Counter(type, "Entries") == 4096 && Counter(type, "Throws") == 43, "repeated unload invokes no callback body");
        using var stats = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.statistics"}"""));
        Check(new[] { "scripts", "coroutines", "bindings" }.All(k => stats.RootElement.GetProperty("data").GetProperty(k).GetUInt64() == 0), "contextual roots released after Closed");
        Console.WriteLine(JsonSerializer.Serialize(new { passed = true, profile = ContextualCleanupWorld.Profile,
            dispatch_phase_invocations = world.DispatchCalls - calls - 1, actual_clr_initialize_entries = 4096,
            actual_clr_cleanup_entries = 4096, real_components = 2048, actual_throws = 43,
            envelope_bytes = Encoding.UTF8.GetByteCount(raw), mutation, managed_integration = false, callback_batch_attestation = false }));
    }
    private static object Fields(int role, bool mutation) => role == 0 && mutation
        ? new { role, sibling = TestGraph.Ref("probe1"), removed = TestGraph.Ref("probe2") } : (object)new { role };
    private static int Counter(Type type, string field) => (int)type.GetField(field, BindingFlags.Public | BindingFlags.Static)!.GetValue(null)!;
    private static void SelectionControls(JsonNode manifest)
    {
        foreach (var field in new[] { "callback_profile", "native_capabilities", "bridge_capabilities" })
        {
            var invalid = manifest.DeepClone();
            invalid["requirements"]!.AsObject().Remove(field);
            Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, invalid.ToJsonString()) != 0, "missing paired contextual requirement rejects before Initialize: " + field);
        }
    }
    private static void MalformedControls(ContextualCleanupWorld world, JsonElement lease, JsonElement envelope, Type type)
    {
        var raw = envelope.GetRawText();
        var last = envelope.GetProperty("callbacks")[4095];
        var record = last.GetRawText();
        var controls = new List<string>
        {
            raw.Replace(record, record.Replace("\"OnDestroy\"", "\"Update\"")),
            ContextualCodecBehavior.Last(raw, "\"payload\":{}", "\"payload\":{\"x\":0}"),
            ContextualCodecBehavior.Last(raw, "\"payload\":{}", "\"payload\":{},\"payl\\u006fad\":{}"),
            Mutate(raw, root => root["callbacks"]!.AsArray().Add(JsonNode.Parse(record))),
            Mutate(raw, root => root["callbacks"]![4095]!["binding"] = "18446744073709551615"),
            Mutate(raw, root => root["callbacks"]![4095]!["component"] = "18446744073709551615"),
            Mutate(raw, root => root["callbacks"]![4095] = root["callbacks"]![4094]!.DeepClone()),
            Mutate(raw, root => root["context"]!["lease"]!["token"] = "forged"),
            Mutate(raw, root => root["context"]!["dynamic_delta"] = 1),
            Mutate(raw, root => root["context"]!["time"]!["numerator"] = 1)
        };
        using var before = Inspect(lease, envelope);
        foreach (var invalid in controls)
        {
            Check(world.Dispatch(invalid) != 0, "malformed contextual envelope rejects");
            using var after = Inspect(lease, envelope);
            Check(before.RootElement.GetRawText() == after.RootElement.GetRawText(), "rejection leaves native cursor/acceptance/envelope unchanged");
            Check(Counter(type, "Entries") == 0, "malformed last/context/4097 executes zero managed callbacks");
        }
        Console.WriteLine("PASS contextual malformed-last/4097/context/generation/binding/kind/payload/duplicate: zero CLR entries and unchanged native cursor");
    }
    private static JsonDocument Inspect(JsonElement lease, JsonElement envelope) => NativeWorld.Request("callbacks.inspect",
        ("context", lease), ("batch", envelope.GetProperty("context").GetProperty("callback_batch")));
    private static string Mutate(string raw, Action<JsonNode> change)
    {
        var root = JsonNode.Parse(raw)!;
        change(root);
        return root.ToJsonString();
    }
    private static void Check(bool condition, string message) => BinderBehavior.Check(condition, message);
}
