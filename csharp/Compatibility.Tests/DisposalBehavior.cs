using System.Collections;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class DisposalBehavior
{
    public static void Run(string nativePath, nint image)
    {
        var create = (delegate* unmanaged[Cdecl]<ulong*, NativeResult>)NativeLibrary.GetExport(image, "disunity_world_create");
        var destroy = (delegate* unmanaged[Cdecl]<ulong, NativeResult>)NativeLibrary.GetExport(image, "disunity_world_destroy");
        var statistics = (delegate* unmanaged[Cdecl]<ulong*, ulong*, NativeResult>)NativeLibrary.GetExport(image, "disunity_store_statistics");
        foreach (var callbacks in new[] { false, true })
        {
            ulong world = 0;
            BinderBehavior.Check(create(&world).code == 0, "disposal regression creates real native world");
            try { Unload(nativePath, world, callbacks); }
            finally
            {
                delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
                unload(); NativeWorld.Detach();
                BinderBehavior.Check(destroy(world).code == 0, "disposal regression destroys host-owned world");
            }
            ulong live = 0, slots = 0;
            BinderBehavior.Check(statistics(&live, &slots).code == 0 && live == 0,
                "throwing managed cleanup leaves zero native worlds after host destruction");
        }
        Console.WriteLine("PASS Cdecl disposal: two throwing iterators, callback failures, original diagnostics, measured zero roots, retry, reload and zero native worlds");
    }
    private static void Unload(string nativePath, ulong world, bool callbacks)
    {
        NativeWorld.AttachNativeLibrary(nativePath, world);
        using var graph = TestGraph.Payload([
            new("first", new GameObject("disposal-first"), "DisposalProbe", new { marker = "first", failCallbacks = callbacks }),
            new("second", new GameObject("disposal-second"), "DisposalProbe", new { marker = "second", failCallbacks = callbacks })]);
        var payload = JsonNode.Parse(graph.RootElement.GetRawText())!;
        payload["world"] = world;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> configure = &ManagedBridge.ConfigureNativeLibrary;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> load = &ManagedBridge.LoadFixture;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> phase = &ManagedBridge.DispatchPhase;
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        BinderBehavior.Check(BridgeBehavior.Call(configure, nativePath) == 0, "configure disposal image");
        BinderBehavior.Check(BridgeBehavior.Call(load, payload.ToJsonString()) == 0, "load disposal fixture: " + BridgeBehavior.LastError());
        BinderBehavior.Check(BridgeBehavior.Call(phase, "{\"phase\":\"Start\"}") == 0, "suspend both throwing iterators");
        using (var before = JsonDocument.Parse(BridgeBehavior.Observe("{\"op\":\"managed.statistics\"}")))
        {
            var data = before.RootElement.GetProperty("data");
            BinderBehavior.Check(data.GetProperty("loaded").GetBoolean() && data.GetProperty("scripts").GetInt32() == 2 &&
                data.GetProperty("coroutines").GetInt32() == 2 && data.GetProperty("bindings").GetInt32() > 0,
                "regression starts with two real suspended script coroutines and measured bindings");
        }
        BinderBehavior.Check(unload() == -1, "first unload honestly reports iterator/callback failures");
        var error = BridgeBehavior.LastError();
        Console.WriteLine("Cdecl disposal diagnostics: " + error);
        AssertDrained();
        foreach (var marker in new[] { "first", "second" })
        {
            BinderBehavior.Check(error.Contains("InvalidOperationException") && error.Contains(marker + " iterator disposal failure"),
                "all original iterator errors remain visible: " + error);
            if (callbacks)
                foreach (var callback in new[] { "OnDisable", "OnDestroy" })
                    BinderBehavior.Check(error.Contains(marker + " " + callback + " failure"), "all callback errors remain visible: " + error);
        }
        BinderBehavior.Check(unload() == 0, "retry succeeds only after honest complete cleanup");
        AssertDrained();
        BinderBehavior.Check(BridgeBehavior.Call(load, payload.ToJsonString()) == 0, "reload works after failed unload cleanup: " + BridgeBehavior.LastError());
        BinderBehavior.Check(unload() == (callbacks ? -1 : 0), "fresh teardown preserves callback failures without retained roots");
        AssertDrained();
    }
    internal static void AssertDrained()
    {
        var json = BridgeBehavior.Observe("{\"op\":\"managed.statistics\"}");
        Console.WriteLine("Cdecl disposal roots: " + json);
        using var result = JsonDocument.Parse(json);
        var data = result.RootElement.GetProperty("data");
        BinderBehavior.Check(!data.GetProperty("loaded").GetBoolean() && data.GetProperty("scripts").GetInt32() == 0 &&
            data.GetProperty("coroutines").GetInt32() == 0 && data.GetProperty("bindings").GetInt32() == 0,
            "all measured managed roots drain even when multiple disposals throw: " + json);
        BinderBehavior.Check(NativeIdentity.Runtime is null && SpriteRenderer.ResolveSprite is null, "runtime and sprite facade hooks are detached");
        try { _ = NativeWorld.Current; throw new Exception("native transport still attached"); }
        catch (InvalidOperationException error) when (error.Message.Contains("require an attached native world")) { }
    }
}

public class DisposalProbe : MonoBehaviour
{
    public string marker = string.Empty;
    public bool failCallbacks = false;
    public static DisposalProbe? Instance;
    private void Awake() => Instance = this;
    private IEnumerator Start() => new ThrowingDisposal(marker);
    private void OnDisable() { if (failCallbacks) throw new ArgumentException(marker + " OnDisable failure"); }
    private void OnDestroy() { if (failCallbacks) throw new NotSupportedException(marker + " OnDestroy failure"); }
    private sealed class ThrowingDisposal(string marker) : IEnumerator, IDisposable
    {
        public object Current => new WaitForSeconds(100);
        public bool MoveNext() => true;
        public void Reset() => throw new NotSupportedException();
        public void Dispose() => throw new InvalidOperationException(marker + " iterator disposal failure");
    }
}
