using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class CallbackFailureBudgetBehavior
{
    public static void Run(string nativePath, ulong world)
    {
        NativeWorld.AttachNativeLibrary(nativePath, world);
        using var graph = TestGraph.Payload([
            new("failure", new GameObject("budget-failure"), "CallbackIsolationProbe", new { failure = "Update", errorText = new string('\u0001', 10000) }),
            new("peer", new GameObject("budget-peer"), "CallbackIsolationProbe", new { })]);
        var payload = JsonNode.Parse(graph.RootElement.GetRawText())!;
        payload["world"] = world;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> configure = &ManagedBridge.ConfigureNativeLibrary;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> load = &ManagedBridge.LoadFixture;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> dispatch = &ManagedBridge.DispatchPhase;
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        BinderBehavior.Check(BridgeBehavior.Call(configure, nativePath) == 0, "configure budget image");
        try
        {
            BinderBehavior.Check(BridgeBehavior.Call(load, payload.ToJsonString()) == 0, "load budget fixture");
            for (var i = 0; i < 4100; i++)
                BinderBehavior.Check(BridgeBehavior.Call(dispatch, """{"phase":"Update"}""") == 0, "overflow keeps transport success and peer delivery");
            var first = BridgeBehavior.Observe("""{"op":"managed.failures","limit":32}""");
            BinderBehavior.Check(Encoding.UTF8.GetByteCount(first) < 3 * 1024 * 1024, "escaped bounded page fits actual 16 MiB response limit");
            using var response = JsonDocument.Parse(first);
            var data = response.RootElement.GetProperty("data");
            BinderBehavior.Check(data.GetProperty("total").GetInt64() == 4100 && data.GetProperty("retained").GetInt32() == 4096 &&
                data.GetProperty("dropped").GetInt64() == 4 && data.GetProperty("records").GetArrayLength() == 32, "overflow is explicit and preserves first 4096 failures");
            BinderBehavior.Check(data.GetProperty("records")[0].GetProperty("message").GetString()!.Length == 4096, "long messages have a strict character cap");
            using var tail = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.failures","after":4090}"""));
            BinderBehavior.Check(tail.RootElement.GetProperty("data").GetProperty("records").GetArrayLength() == 6, "bounded pagination reaches final retained failures");
            using var worldSnapshot = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"world.snapshot"}"""));
            var peer = worldSnapshot.RootElement.GetProperty("data").GetProperty("objects").EnumerateArray().Single(o => o.GetProperty("name").GetString() == "budget-peer");
            BinderBehavior.Check(peer.GetProperty("transform").GetProperty("scale")[1].GetSingle() == 4101, "overflow never skips eligible peer callbacks");
            RejectOversizedPage();
        }
        finally { BinderBehavior.Check(unload() == 0, "budget evidence has no managed script roots"); }
        Console.WriteLine("PASS callback failure budgets: 4096 retained, explicit overflow, 32-record pure Cdecl pages, bounded escaped text and 4100 peer Updates");
    }
    private static void RejectOversizedPage()
    {
        delegate* unmanaged[Cdecl]<byte*, nuint, byte*, nuint, nuint*, int> observe = &ManagedBridge.Observe;
        var query = Encoding.UTF8.GetBytes("""{"op":"managed.failures","limit":33}""");
        fixed (byte* q = query)
        {
            nuint size = 0;
            BinderBehavior.Check(observe(q, (nuint)query.Length, null, 0, &size) == -1 &&
                BridgeBehavior.LastError().Contains("limit 1..32"), "over-budget failure query rejects before copy");
        }
    }
}
