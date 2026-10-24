using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class CallbackIsolationBehavior
{
    internal static void Run(string nativePath, ulong world)
    {
        var phases = Environment.GetEnvironmentVariable("CALLBACK_TEST_PHASE") is { } selected
            ? new[] { selected } : new[] { "Awake", "OnEnable", "Start", "FixedUpdate", "Update", "LateUpdate" };
        foreach (var phase in phases) Phase(nativePath, world, phase);
        Console.WriteLine("PASS callback isolation: Cdecl initialization/start/update/fixed/late peers, structured original errors and pure probes");
    }
    private static void Phase(string nativePath, ulong world, string phase)
    {
        NativeWorld.AttachNativeLibrary(nativePath, world);
        var first = new GameObject("failure-" + phase);
        var peer = new GameObject("peer-" + phase);
        using var graph = TestGraph.Payload([
            new("failure", first, "CallbackIsolationProbe", new { failure = phase }),
            new("peer", peer, "CallbackIsolationProbe", new { })]);
        var payload = JsonNode.Parse(graph.RootElement.GetRawText())!;
        payload["world"] = world;
        var component = payload["handles"]!["failure"]!.GetValue<ulong>();
        delegate* unmanaged[Cdecl]<byte*, nuint, int> configure = &ManagedBridge.ConfigureNativeLibrary;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> load = &ManagedBridge.LoadFixture;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> dispatch = &ManagedBridge.DispatchPhase;
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        BinderBehavior.Check(BridgeBehavior.Call(configure, nativePath) == 0, "configure callback provider");
        try
        {
            BinderBehavior.Check(BridgeBehavior.Call(load, payload.ToJsonString()) == 0, "consumed initialization returns transport OK: " + BridgeBehavior.LastError());
            var dispatchPhase = phase is "Awake" or "OnEnable" ? "Update" : phase;
            BinderBehavior.Check(BridgeBehavior.Call(dispatch, JsonSerializer.Serialize(new { phase = dispatchPhase })) == 0,
                "consumed callback batch returns transport OK: " + BridgeBehavior.LastError());
            AssertPeer(phase, dispatchPhase);
            using var trace = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.public","type":"CallbackIsolationProbe","id":"peer","member":"callbackTrace"}"""));
            var expectedTrace = "Awake>OnEnable>Start>" + (dispatchPhase == "Start" ? string.Empty : dispatchPhase + ">");
            BinderBehavior.Check(trace.RootElement.GetProperty("data").GetString() == expectedTrace, "per-instance lifecycle partial order follows G02.T01 baseline");
            var before = BridgeBehavior.Observe("{\"op\":\"managed.failures\"}");
            BinderBehavior.Check(before == BridgeBehavior.Observe("{\"op\":\"managed.failures\"}"), "failure sizing/copy never drains evidence");
            using var failures = JsonDocument.Parse(before);
            var data = failures.RootElement.GetProperty("data");
            BinderBehavior.Check(data.GetProperty("total").GetInt64() == 1, "one callback failure, no retries");
            var error = data.GetProperty("records")[0];
            AssertError(error, phase, component);
            BinderBehavior.Check(BridgeBehavior.Call(dispatch, "{\"phase\":\"Unsupported\"}") == -1, "unexpected phase remains entry error");
            var lastError = BridgeBehavior.LastError();
            _ = BridgeBehavior.Observe("{\"op\":\"managed.failures\"}");
            BinderBehavior.Check(BridgeBehavior.LastError() == lastError, "Observe probes/copies preserve entry diagnostics");
            BinderBehavior.Check(BridgeBehavior.Call(dispatch, "{\"phase\":\"Start\"}") == 0, "Start is not retried after failure");
            using var after = JsonDocument.Parse(BridgeBehavior.Observe("{\"op\":\"managed.failures\"}"));
            BinderBehavior.Check(after.RootElement.GetProperty("data").GetProperty("total").GetInt64() == 1, "once-only failed lifecycle is not reexecuted");
        }
        finally { BinderBehavior.Check(unload() == 0, "callback failures do not become teardown failures"); }
    }
    private static void AssertPeer(string phase, string dispatchPhase)
    {
            var snapshot = JsonDocument.Parse(BridgeBehavior.Observe("{\"op\":\"world.snapshot\"}"));
            using (snapshot)
            {
                var row = snapshot.RootElement.GetProperty("data").GetProperty("objects").EnumerateArray()
                    .Single(o => o.GetProperty("name").GetString() == "peer-" + phase);
                var p = row.GetProperty("transform").GetProperty("position");
                BinderBehavior.Check(p[0].GetSingle() == 1 && p[1].GetSingle() == 1 && p[2].GetSingle() == 1,
                    "eligible peer initializes and starts exactly once after " + phase + " failure");
                var scale = row.GetProperty("transform").GetProperty("scale");
                var axis = dispatchPhase == "FixedUpdate" ? 0 : dispatchPhase == "LateUpdate" ? 2 : 1;
                if (dispatchPhase != "Start") BinderBehavior.Check(scale[axis].GetSingle() == 2, "peer receives actual " + dispatchPhase + " callback");
            }
    }
    internal static void AssertError(JsonElement error, string message, ulong component)
    {
        var expected = Environment.GetEnvironmentVariable("CALLBACK_EXPECT_WRONG_ERROR") is null ? "callback sentinel " + message : "deliberately wrong";
        BinderBehavior.Check(error.GetProperty("exception_type").GetString() == "System.ArgumentException" &&
            error.GetProperty("message").GetString() == expected, "original callback type/message preserved");
        BinderBehavior.Check(error.GetProperty("stack").GetString()!.Contains("CallbackIsolationProbe." + message), "original callback stack preserved");
        BinderBehavior.Check(error.GetProperty("component").GetUInt64() == component && error.GetProperty("callback").GetString() == message,
            "failure identifies actual component and message");
        var phase = message is "Awake" or "OnEnable" ? "Initialize" : message;
        BinderBehavior.Check(error.GetProperty("phase").GetString() == phase && error.GetProperty("frame").ValueKind == JsonValueKind.Null,
            "failure uses actual entry phase and leaves unavailable frame unknown");
    }
    internal static void Contact()
    {
        var zone = new GameObject("throwing-contact");
        zone.AddComponent<BoxCollider2D>().isTrigger = true;
        var ball = new GameObject("contact-peer");
        ball.AddComponent<CircleCollider2D>();
        ball.AddComponent<Rigidbody2D>().gravityScale = 0;
        using var graph = TestGraph.Payload([
            new("throw", zone, "IsolatedContactProbe", new { fail = true }),
            new("same-owner", zone, "IsolatedContactProbe", new { }),
            new("other-owner", ball, "IsolatedContactProbe", new { })]);
        using var session = new ScriptSession(graph.RootElement);
        session.Initialize();
        NativeWorld.Request("physics.step", ("dt", 0.02f)).Dispose();
        using var events = NativeWorld.Request("events.read");
        var enter = NativeWorld.Data(events).EnumerateArray().Single(e => e.GetProperty("phase").GetString() == "enter");
        session.InvokeContact(enter);
        BinderBehavior.Check(zone.transform.position.z == 1 && ball.transform.position.z == 1,
            "real solver contact continues same-owner and opposite-owner peers after failure");
        using var query = JsonDocument.Parse("{\"op\":\"managed.failures\"}");
        using var failures = JsonDocument.Parse(JsonSerializer.Serialize(session.Observe(query.RootElement)));
        var record = failures.RootElement.GetProperty("data").GetProperty("records")[0];
        BinderBehavior.Check(record.GetProperty("phase").GetString() == "contacts.enter" &&
            record.GetProperty("callback").GetString() == "OnTriggerEnter2D" &&
            record.GetProperty("message").GetString() == "solver contact sentinel", "actual contact phase/error preserved");
        Console.WriteLine("PASS callback isolation: actual native solver contacts continue eligible peers");
    }
}

public class CallbackIsolationProbe : MonoBehaviour
{
    public string failure = string.Empty;
    public string errorText = string.Empty;
    public string callbackTrace = string.Empty;
    private void Check(string message)
    {
        callbackTrace += message + ">";
        if (failure == message) throw new ArgumentException(errorText.Length == 0 ? "callback sentinel " + message : errorText);
    }
    private void Awake() { Check("Awake"); var p = transform.position; p.x++; transform.position = p; }
    private void OnEnable() { Check("OnEnable"); var p = transform.position; p.y++; transform.position = p; }
    private void Start() { Check("Start"); var p = transform.position; p.z++; transform.position = p; }
    private void FixedUpdate() { Check("FixedUpdate"); var p = transform.localScale; p.x++; transform.localScale = p; }
    private void Update() { Check("Update"); var p = transform.localScale; p.y++; transform.localScale = p; }
    private void LateUpdate() { Check("LateUpdate"); var p = transform.localScale; p.z++; transform.localScale = p; }
}
public class IsolatedContactProbe : MonoBehaviour
{
    public bool fail = false;
    private void OnTriggerEnter2D(Collider2D other)
    {
        if (fail) throw new InvalidOperationException("solver contact sentinel");
        var p = transform.position; p.z++; transform.position = p;
    }
}
