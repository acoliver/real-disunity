using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class ByteLimitsBehavior
{
    internal static void Descriptor()
    {
        delegate* unmanaged[Cdecl]<byte*, nuint, nuint*, int> describe = &ManagedBridge.DescribeBridge;
        nuint length = 0;
        BinderBehavior.Check(describe(null, 0, &length) == -2, "byte descriptor probe");
        var bytes = new byte[checked((int)length)];
        fixed (byte* output = bytes) BinderBehavior.Check(describe(output, length, &length) == 0, "byte descriptor copy");
        using var json = JsonDocument.Parse(bytes);
        BinderBehavior.Check(json.RootElement.GetProperty("capabilities").EnumerateArray().Any(v => v.GetString() == "byte_limits"), "bridge must advertise baseline byte limits");
        Console.WriteLine("PASS baseline byte limits bridge capability and pure Cdecl descriptor");
    }

    internal static void Run(string path)
    {
        Descriptor();
        using var world = new ReadinessWorld(path);
        var manifest = world.Manifest([new("probe", new GameObject("byte-probe"), nameof(ReadinessProbe), new { })]);
        NativeIdentity.Reset();
        BinderBehavior.Check(world.Load(manifest) == 0, "baseline byte limits Initialize: " + BridgeBehavior.LastError());
        world.Frame();
        using var response = NativeWorld.Request("transport.get");
        var effective = NativeWorld.Data(response);
        BinderBehavior.Check(effective.GetProperty("limits").GetProperty("max_response_bytes").GetInt32() == 8 << 20, "effective native reply budget");
        var oversized = new string(' ', (1 << 20) + 1);
        var before = BridgeBehavior.Observe("""{"op":"managed.snapshot"}""");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, oversized) != 0, "oversize Cdecl request rejected before dispatch");
        BinderBehavior.Check(before == BridgeBehavior.Observe("""{"op":"managed.snapshot"}"""), "oversize request executed no callbacks");
        for (var i = 0; i < 1100; i++) NativeWorld.Request("time.get").Dispose();
        world.Cleanup();
        Console.WriteLine("PASS byte consumer actual native strict Initialize/frame/Cleanup, 1MiB+1 Cdecl predispatch rejection, 1100 synchronous retained-response releases");
    }

}
