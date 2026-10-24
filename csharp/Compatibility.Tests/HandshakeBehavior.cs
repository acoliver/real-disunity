using System.Reflection;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class HandshakeBehavior
{
    public static void Describe()
    {
        var method = typeof(ManagedBridge).GetMethod("DescribeBridge", BindingFlags.Public | BindingFlags.Static);
        BinderBehavior.Check(method is not null, "missing pure DescribeBridge Cdecl entry");
        var entry = (delegate* unmanaged[Cdecl]<byte*, nuint, nuint*, int>)method!.MethodHandle.GetFunctionPointer();
        delegate* unmanaged[Cdecl]<byte*, nuint, int> dispatch = &ManagedBridge.DispatchPhase;
        BridgeBehavior.Call(dispatch, "{}");
        var error = BridgeBehavior.LastError();
        var before = BridgeBehavior.Observe("""{"op":"managed.statistics"}""");
        BridgeBehavior.Call(dispatch, "{}");
        var bytes = Copy(entry);
        BinderBehavior.Check(BridgeBehavior.LastError() == error, "DescribeBridge must preserve last error");
        BinderBehavior.Check(bytes == Copy(entry), "DescribeBridge copy retry must be stable");
        BinderBehavior.Check(entry(null, 0, null) != 0, "DescribeBridge null length is rejected");
        BinderBehavior.Check(before == BridgeBehavior.Observe("""{"op":"managed.statistics"}"""), "DescribeBridge must not create managed roots");
        using var json = JsonDocument.Parse(bytes);
        var root = json.RootElement;
        BinderBehavior.Check(root.GetProperty("bridge_revision").GetInt32() == 1, "bridge revision");
        BinderBehavior.Check(root.GetProperty("pointer_width").GetInt32() == IntPtr.Size * 8, "nuint pointer width");
        BinderBehavior.Check(root.GetProperty("clock_policy").GetString() == "float_accumulated", "bridge must not claim rational clock");
        var capabilities = root.GetProperty("capabilities").EnumerateArray().Select(c => c.GetString()).ToArray();
        BinderBehavior.Check(capabilities.Contains("native_phase_context_v1"), "bridge phase-context consumer capability");
        BinderBehavior.Check(!capabilities.Contains("phase_tokens") && !capabilities.Contains("native_scheduler_v1"), "bridge must preserve bounded capability claims");
        BinderBehavior.Check(!root.GetProperty("native_scheduler").GetBoolean() && root.GetProperty("coroutine_readiness").GetString() == "managed", "managed scheduler authority must remain explicit");
        BinderBehavior.Check(capabilities.Contains("native_readiness_single_v1"), "bridge bounded readiness consumer capability");
        var bounded = root.GetProperty("scheduler_profiles")[0];
        BinderBehavior.Check(bounded.GetProperty("iterator_model").GetString() == "single_iterator_per_routine"
            && !bounded.GetProperty("nested_stacks").GetBoolean() && !bounded.GetProperty("predicates").GetBoolean()
            && !bounded.GetProperty("immediate_start_inside_ticket").GetBoolean(), "bounded readiness descriptor preserves unsupported semantics");
        BinderBehavior.Check(!bytes.Contains("host_key", StringComparison.Ordinal), "bridge profile must not expose private authority");
        Console.WriteLine("PASS pure DescribeBridge Cdecl probe/copy/retry/no-partial-write and root/error purity");
    }
    private static string Copy(delegate* unmanaged[Cdecl]<byte*, nuint, nuint*, int> entry)
    {
        nuint length = 0;
        BinderBehavior.Check(entry(null, 0, &length) == -2 && length > 0, "DescribeBridge zero-byte probe");
        var bytes = Enumerable.Repeat((byte)0xA5, checked((int)length)).ToArray();
        fixed (byte* output = bytes)
        {
            BinderBehavior.Check(entry(output, length - 1, &length) == -2 && bytes.All(b => b == 0xA5), "DescribeBridge short copy writes nothing");
            BinderBehavior.Check(entry(output, length, &length) == 0, "DescribeBridge full copy");
        }
        return Encoding.UTF8.GetString(bytes);
    }
    public static void RejectBeforeAwake(string nativePath, ulong world)
    {
        var witness = new GameObject("handshake-witness");
        using var graph = TestGraph.Payload([new("probe", witness, "HandshakeProbe", new { })]);
        var payload = JsonNode.Parse(graph.RootElement.GetRawText())!;
        payload["world"] = world;
        payload["version"] = 1;
        payload["schema_version"] = 1;
        payload["requirements"] = JsonNode.Parse("""{"native_abi_version":2,"native_protocol_version":1,"bridge_revision":1,"manifest_schema_version":1,"pointer_width":64,"native_capabilities":["describe"],"bridge_capabilities":["describe_bridge"]}""");
        delegate* unmanaged[Cdecl]<byte*, nuint, int> configure = &ManagedBridge.ConfigureNativeLibrary;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> load = &ManagedBridge.LoadFixture;
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        BinderBehavior.Check(BridgeBehavior.Call(configure, nativePath) == 0, "configure paired image");
        foreach (var invalid in InvalidRequirements())
        {
            var bad = payload.DeepClone();
            foreach (var item in invalid) bad[item.Key] = item.Value?.DeepClone();
            var code = BridgeBehavior.Call(load, bad.ToJsonString());
            using var position = NativeWorld.Request("transform.get", ("object", witness.NativeHandle));
            BinderBehavior.Check(NativeWorld.Data(position).GetProperty("position")[0].GetSingle() == 0, "incompatible manifest ran Awake before handshake rejection");
            BinderBehavior.Check(code != 0, "incompatible manifest must reject");
            BinderBehavior.Check(NativeIdentity.Runtime is null, "rejection must leave script runtime detached");
        }
        BinderBehavior.Check(BridgeBehavior.Call(load, "{") != 0, "malformed manifest rejects");
        BinderBehavior.Check(BridgeBehavior.Call(load, payload.ToJsonString()) == 0, "compatible versioned manifest loads: " + BridgeBehavior.LastError());
        BinderBehavior.Check(witness.transform.position.x == 99, "compatible manifest activates actual private Awake");
        Describe();
        BinderBehavior.Check(unload() == 0, "handshake test unload");
        Console.WriteLine("PASS manifest negotiation rejects before private Awake; compatible actual source Cdecl activation");
    }
    private static IEnumerable<JsonObject> InvalidRequirements()
    {
        foreach (var text in new[] {
            """{"version":2}""", """{"version":"1"}""", """{"schema_version":2}""",
            """{"requirements":{"native_abi_version":99}}""",
            """{"requirements":{"native_protocol_version":2}}""",
            """{"requirements":{"bridge_revision":2}}""",
            """{"requirements":{"pointer_width":32}}""",
            """{"requirements":{"endianness":"other"}}""",
            """{"requirements":{"clock_policy":"rational"}}""",
            """{"requirements":{"native_capabilities":["phase_tokens"]}}""",
            """{"requirements":{"bridge_capabilities":["phase_tokens"]}}""",
            """{"requirements":{"unknown":1}}""", """{"requirements":null}""",
            """{"world":0}""", """{"world":1}"""
        }) yield return JsonNode.Parse(text)!.AsObject();
    }
}
public sealed class HandshakeProbe : MonoBehaviour
{
    private void Awake() => transform.position = new Vector3(99, 0, 0);
}
