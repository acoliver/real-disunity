using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal sealed unsafe class ContextualCleanupWorld : IDisposable
{
    internal const string Profile = "native_contextual_lifecycle_batch_v1";
    private readonly nint _image;
    private readonly WorldDestroy _destroy;
    private readonly JsonElement _host;
    internal ulong World { get; }
    internal int DispatchCalls { get; private set; }
    internal ContextualCleanupWorld(string path)
    {
        _image = NativeLibrary.Load(path);
        var create = Marshal.GetDelegateForFunctionPointer<WorldCreate>(NativeLibrary.GetExport(_image, "disunity_world_create"));
        _destroy = Marshal.GetDelegateForFunctionPointer<WorldDestroy>(NativeLibrary.GetExport(_image, "disunity_world_destroy"));
        BinderBehavior.Check(create(out var world).code == 0, "contextual world create");
        World = world;
        NativeWorld.AttachNativeLibrary(path, World);
        using var opened = NativeWorld.Request("session.open", ("profile", Profile));
        var data = NativeWorld.Data(opened);
        _host = JsonSerializer.SerializeToElement(new { session = data.GetProperty("session").GetString(), host_key = data.GetProperty("host_key").GetString() });
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.ConfigureNativeLibrary, path) == 0, "contextual configure");
    }
    internal JsonNode Manifest(ScriptSpec[] scripts)
    {
        using var payload = TestGraph.Payload(scripts);
        var manifest = JsonNode.Parse(payload.RootElement.GetRawText())!;
        manifest["world"] = World;
        manifest["requirements"] = JsonNode.Parse("""{"scheduler_profile":"native_contextual_lifecycle_batch_v1","execution_profile":"native_phase_context_v1","callback_profile":"contextual_lifecycle_batch_v1","native_capabilities":["native_contextual_lifecycle_batch_v1"],"bridge_capabilities":["managed_contextual_cleanup_batch_consumer_v1"]}""");
        return manifest;
    }
    internal void Load(JsonNode manifest)
    {
        var context = Open("Initialize");
        manifest["initial_context"] = JsonNode.Parse(context.GetRawText());
        NativeIdentity.Reset();
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, manifest.ToJsonString()) == 0, "contextual load: " + BridgeBehavior.LastError());
        Finish(context);
    }
    internal JsonElement Open(string phase)
    {
        using var opened = Control("phase.open", ("phase", phase), ("frame", 0UL), ("fixed_tick", 0UL));
        return NativeWorld.Data(opened).GetProperty("context").Clone();
    }
    internal JsonElement Reserve(JsonElement context)
    {
        NativeWorld.Request("phase.claim", ("phase", "Cleanup"), ("context", context)).Dispose();
        using var reserved = NativeWorld.Request("callbacks.next", ("context", context));
        return NativeWorld.Data(reserved).Clone();
    }
    internal int Dispatch(string raw)
    {
        DispatchCalls++;
        return BridgeBehavior.Call(&ManagedBridge.DispatchPhase, raw);
    }
    internal void Finish(JsonElement context) => Control("phase.finish", ("context", context)).Dispose();
    internal JsonDocument Control(string op, params (string key, object? value)[] fields) => NativeWorld.Request(op, fields.Append(("host", (object?)_host)).ToArray());
    public void Dispose()
    {
        NativeWorld.Detach();
        BinderBehavior.Check(_destroy(World).code == 0, "contextual world destroy");
        NativeLibrary.Free(_image);
    }
}
