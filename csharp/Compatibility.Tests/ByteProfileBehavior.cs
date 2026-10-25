using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class ByteProfileBehavior
{
    internal static void Descriptor()
    {
        delegate* unmanaged[Cdecl]<byte*, nuint, nuint*, int> describe = &ManagedBridge.DescribeBridge;
        nuint length = 0;
        BinderBehavior.Check(describe(null, 0, &length) == -2, "byte descriptor probe");
        var bytes = new byte[checked((int)length)];
        fixed (byte* output = bytes) BinderBehavior.Check(describe(output, length, &length) == 0, "byte descriptor copy");
        using var json = JsonDocument.Parse(bytes);
        BinderBehavior.Check(json.RootElement.GetProperty("capabilities").EnumerateArray().Any(v => v.GetString() == "managed_transport_profile_consumer_v1"), "bridge must advertise its separate managed byte-profile consumer capability");
        Console.WriteLine("PASS byte consumer distinct bridge capability and pure Cdecl descriptor");
    }

    internal static void Run(string path)
    {
        Descriptor();
        using var world = new ReadinessWorld(path, byteProfile: true);
        var manifest = world.Manifest([new("probe", new GameObject("byte-probe"), nameof(ReadinessProbe), new { })]);
        manifest["requirements"]!["transport_profile"] = "transport_byte_limits_v1";
        NativeIdentity.Reset();
        var missing = manifest.DeepClone();
        missing["requirements"]!.AsObject().Remove("transport_profile");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, missing.ToJsonString()) != 0, "native selected byte profile requires explicit matching manifest before binding");
        AssertNoRoots();
        BinderBehavior.Check(world.Load(manifest) == 0, "actual selected byte consumer Initialize: " + BridgeBehavior.LastError());
        world.Frame();
        using var response = NativeWorld.Request("transport.get");
        var effective = NativeWorld.Data(response);
        BinderBehavior.Check(effective.GetProperty("limits").GetProperty("max_response_bytes").GetInt32() == 8 << 20, "effective native reply budget");
        BinderBehavior.Check(!effective.GetProperty("managed_integration").GetBoolean(), "native-only qualification flag remains unchanged");
        var oversized = new string(' ', (1 << 20) + 1);
        var before = BridgeBehavior.Observe("""{"op":"managed.snapshot"}""");
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.DispatchPhase, oversized) != 0, "oversize Cdecl request rejected before dispatch");
        BinderBehavior.Check(before == BridgeBehavior.Observe("""{"op":"managed.snapshot"}"""), "oversize request executed no callbacks");
        for (var i = 0; i < 1100; i++) NativeWorld.Request("time.get").Dispose();
        world.Cleanup();
        Console.WriteLine("PASS byte consumer actual native strict Initialize/frame/Cleanup, 1MiB+1 Cdecl predispatch rejection, 1100 synchronous retained-response releases");
    }

    internal static void Unselected(string path)
    {
        using var world = new ReadinessWorld(path);
        using var before = NativeWorld.Request("snapshot");
        var rejected = false;
        try { NativeWorld.AttachNativeByteProfile(path, world.World); }
        catch (InvalidDataException error) { rejected = error.Message.Contains("byte transport"); }
        BinderBehavior.Check(rejected, "facade cannot consume unselected native transport");
        using var after = NativeWorld.Request("snapshot");
        BinderBehavior.Check(before.RootElement.GetRawText() == after.RootElement.GetRawText(), "failed facade opt-in is a pure query with no native mutation");
        var manifest = world.Manifest([new("probe", new GameObject("byte-unselected"), nameof(ReadinessProbe), new { })]);
        manifest["requirements"]!["transport_profile"] = "transport_byte_limits_v1";
        NativeIdentity.Reset();
        BinderBehavior.Check(BridgeBehavior.Call(&ManagedBridge.LoadFixture, manifest.ToJsonString()) != 0, "byte requirement cannot activate native selection");
        BinderBehavior.Check(BridgeBehavior.LastError().Contains("byte transport"), "specific byte transport mismatch diagnosis");
        AssertNoRoots();
        Console.WriteLine("PASS byte consumer native unselected image rejects before Initialize/binding");
    }

    internal static void RejectMissingRequirement(string path)
    {
        using var world = new ReadinessWorld(path, nativeOnlyByteProfile: true);
        var manifest = world.Manifest([new("probe", new GameObject("native-byte-no-requirement"), nameof(ReadinessProbe), new { })]);
        NativeIdentity.Reset();
        BinderBehavior.Check(world.Load(manifest) != 0, "native byte-selected fixture without explicit transport requirement must reject before Initialize");
        AssertNoRoots();
        Console.WriteLine("PASS byte consumer actual native selected but missing manifest requirement rejects before Initialize/binding");
    }

    private static void AssertNoRoots()
    {
        using var stats = JsonDocument.Parse(BridgeBehavior.Observe("""{"op":"managed.statistics"}"""));
        var data = stats.RootElement.GetProperty("data");
        BinderBehavior.Check(new[] { "scripts", "coroutines", "bindings" }.All(k => data.GetProperty(k).GetUInt64() == 0), "byte mismatch retains no managed roots");
    }
}
