using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal sealed unsafe class ReadinessWorld : IDisposable
{
    private readonly WorldDestroy _destroy;
    private readonly JsonElement _host;
    private readonly bool _nestedSession;
    internal ulong World { get; }
    internal string Path { get; }
    internal ReadinessWorld(string path, bool openSession = true, bool nestedSession = false, Action? beforeOpen = null)
    {
        Path = path;
        _nestedSession = nestedSession;
        var image = NativeLibrary.Load(path);
        var create = Marshal.GetDelegateForFunctionPointer<WorldCreate>(NativeLibrary.GetExport(image, "disunity_world_create"));
        _destroy = Marshal.GetDelegateForFunctionPointer<WorldDestroy>(NativeLibrary.GetExport(image, "disunity_world_destroy"));
        BinderBehavior.Check(create(out var world).code == 0, "readiness world creation");
        World = world;
        NativeWorld.AttachNativeLibrary(path, World);
        if (!openSession)
        {
            BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.ConfigureNativeLibrary, path) == 0, "configure byte boundary provider");
            return;
        }
        beforeOpen?.Invoke();
        using var opened = NativeWorld.Request("session.open", ("profile", nestedSession ? "native_readiness_nested_start_v1" : "native_phase_context_v1"));
        var data = NativeWorld.Data(opened);
        _host = JsonSerializer.SerializeToElement(new { session = data.GetProperty("session").GetString(), host_key = data.GetProperty("host_key").GetString() });
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.ConfigureNativeLibrary, path) == 0, "configure readiness provider");
    }
    internal JsonNode Manifest(ScriptSpec[] scripts)
    {
        using var payload = TestGraph.Payload(scripts);
        var manifest = JsonNode.Parse(payload.RootElement.GetRawText())!;
        manifest["world"] = World;
        manifest["requirements"] = JsonNode.Parse("""{"execution_profile":"native_phase_context_v1","scheduler_profile":"native_readiness_single_v1"}""");
        if (_nestedSession) manifest["requirements"]!["scheduler_profile"] = "native_readiness_nested_start_v1";
        return manifest;
    }
    internal int Load(JsonNode manifest)
    {
        var context = Open("Initialize");
        manifest["initial_context"] = JsonNode.Parse(context.GetRawText());
        var result = BridgeBehavior.Call(&ManagedBridge.LoadFixture, manifest.ToJsonString());
        if (result == 0) Finish(context);
        return result;
    }
    internal JsonElement Open(string phase)
    {
        using var status = NativeWorld.Request("session.status");
        var data = NativeWorld.Data(status);
        var tick = data.GetProperty("fixed_tick").GetUInt64() + (phase is "FixedUpdate" or "FixedCoroutines" ? 1UL : 0UL);
        using var opened = Control("phase.open", ("phase", phase), ("frame", data.GetProperty("frame").GetUInt64()), ("fixed_tick", tick));
        return NativeWorld.Data(opened).GetProperty("context").Clone();
    }
    internal void Finish(JsonElement context) => Control("phase.finish", ("context", context)).Dispose();
    internal void Phase(string phase)
    {
        var context = Open(phase);
        var payload = JsonSerializer.Serialize(new { phase, context });
        var result = phase == "Contacts"
            ? BridgeBehavior.Call(&ManagedBridge.InvokeContact, payload)
            : BridgeBehavior.Call(&ManagedBridge.DispatchPhase, payload);
        BinderBehavior.Check(result == 0, "readiness phase " + phase + ": " + BridgeBehavior.LastError());
        if (phase == "Cleanup") NativeWorld.AttachNativeLibrary(Path, World);
        Finish(context);
    }
    internal void Frame(ulong num = 0, ulong den = 1)
    {
        using var opened = Control("frame.open", ("dt", new { num, den }));
        var steps = NativeWorld.Data(opened).GetProperty("time").GetProperty("fixed_steps").GetInt32();
        for (var i = 0; i < steps; i++)
        {
            Phase("FixedUpdate"); Phase("FixedCoroutines");
            using var status = NativeWorld.Request("session.status");
            var data = NativeWorld.Data(status);
            Control("fixed.commit", ("frame", data.GetProperty("frame").GetUInt64()), ("fixed_tick", data.GetProperty("fixed_tick").GetUInt64() + 1)).Dispose();
            Phase("Contacts");
        }
        foreach (var phase in new[] { "Update", "UpdateCoroutines", "LateUpdate", "EndOfFrame" }) Phase(phase);
        Control("frame.end", ("frame", NativeWorld.Data(opened).GetProperty("time").GetProperty("frame").GetUInt64())).Dispose();
    }
    internal JsonDocument Control(string op, params (string key, object? value)[] fields) =>
        NativeWorld.Request(op, fields.Append(("host", (object?)_host)).ToArray());
    internal void Cleanup()
    {
        Phase("Cleanup");
        NativeWorld.AttachNativeLibrary(Path, World);
        using var status = NativeWorld.Request("session.status");
        BinderBehavior.Check(NativeWorld.Data(status).GetProperty("stage").GetString() == "Closed", "Cleanup closes native session");
        using var stats = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.statistics"}"""));
        var data = stats.RootElement.GetProperty("data");
        BinderBehavior.Check(new[] { "scripts", "coroutines", "bindings" }.All(k => data.GetProperty(k).GetUInt64() == 0), "Cleanup drains managed roots");
    }
    public void Dispose()
    {
        NativeWorld.Detach();
        BinderBehavior.Check(_destroy(World).code == 0, "readiness native world destroyed");
    }
}
