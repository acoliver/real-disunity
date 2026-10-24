using System.Runtime.InteropServices;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static unsafe class BridgeBehavior
{
    public static void BeforeLoad()
    {
        using var result = JsonDocument.Parse(Observe("""{"op":"managed.statistics"}"""));
        var data = result.RootElement.GetProperty("data");
        BinderBehavior.Check(!data.GetProperty("loaded").GetBoolean() && data.GetProperty("scripts").GetUInt64() == 0 && data.GetProperty("coroutines").GetUInt64() == 0 && data.GetProperty("bindings").GetUInt64() == 0, "statistics works before the first fixture exists");
    }
    public static void Run(string repository, string nativePath, ulong world)
    {
        var witness = new GameObject("bridge-witness");
        using var graph = TestGraph.Payload([new("probe", witness, "BridgeLifecycleProbe", new { })]);
        var payload = JsonNode.Parse(graph.RootElement.GetRawText())!;
        payload["world"] = world;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> configure = &ManagedBridge.ConfigureNativeLibrary;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> load = &ManagedBridge.LoadFixture;
        delegate* unmanaged[Cdecl]<byte*, nuint, int> dispatch = &ManagedBridge.DispatchPhase;
        delegate* unmanaged[Cdecl]<int> unload = &ManagedBridge.Unload;
        BinderBehavior.Check(Call(configure, nativePath) == 0, "bridge config must select paired native image");
        var badPayload = JsonNode.Parse(payload.ToJsonString())!;
        badPayload["assemblies"] = new JsonArray(Path.Combine(repository, ".work/managed/missing-game.dll"));
        BinderBehavior.Check(Call(load, badPayload.ToJsonString()) != 0 && LastError().Contains("fixture assembly not found"), "missing game assembly fails load with exact diagnostic");
        NativeWorld.AttachNativeLibrary(nativePath, world);
        BinderBehavior.Check(Call(load, payload.ToJsonString()) == 0, "failed load cleans roots and permits real load: " + LastError());
        BinderBehavior.Check(Call(load, payload.ToJsonString()) != 0 && LastError().Contains("already loaded"), "duplicate fixture load must reject");
        var before = Observe("{\"op\":\"managed.snapshot\"}");
        var again = Observe("{\"op\":\"managed.snapshot\"}");
        BinderBehavior.Check(before == again, "observation probes are repeatable and side-effect free");
        using (var observation = JsonDocument.Parse(before)) BinderBehavior.Check(observation.RootElement.GetProperty("data").GetProperty("scripts").GetInt32() == 1, "Observe reports actual bound scripts");
        using (var value = JsonDocument.Parse(Observe("""{"op":"managed.query","type":"BridgeLifecycleProbe","member":"CurrentX"}""")))
            BinderBehavior.Check(value.RootElement.GetProperty("data").GetInt32() == 0, "Cdecl public getter before native dispatch");
        BinderBehavior.Check(Call(dispatch, "{\"phase\":\"Update\"}") == 0, "Cdecl dispatch runs bound private script");
        using (var value = JsonDocument.Parse(Observe("""{"op":"managed.public","type":"BridgeLifecycleProbe","path":"CurrentX"}""")))
            BinderBehavior.Check(value.RootElement.GetProperty("data").GetInt32() == 1, "Cdecl public getter observes current native value without dispatch");
        using (var snapshot = JsonDocument.Parse(Observe("{\"op\":\"world.snapshot\"}")))
        {
            var row = snapshot.RootElement.GetProperty("data").GetProperty("objects").EnumerateArray().Single(o => o.GetProperty("name").GetString() == "bridge-witness");
            BinderBehavior.Check(row.GetProperty("transform").GetProperty("position")[0].GetSingle() == 1, "private bridge-dispatched Update has native effect");
        }
        BinderBehavior.Check(Call(dispatch, "{\"phase\":\"Unsupported\"}") != 0 && LastError().Contains("DispatchPhase('Unsupported')"), "unsupported phase surfaces exact diagnostic");
        using (var statistics = JsonDocument.Parse(Observe("""{"op":"managed.statistics"}""")))
        {
            var data = statistics.RootElement.GetProperty("data");
            BinderBehavior.Check(data.GetProperty("loaded").GetBoolean() && data.GetProperty("scripts").GetUInt64() == 1 && data.GetProperty("coroutines").GetUInt64() == 1 && data.GetProperty("bindings").GetUInt64() > 0, "loaded statistics measure actual script and binding registries");
        }
        BinderBehavior.Check(unload() == 0, "Cdecl unload succeeds");
        var drained = Observe("""{"op":"managed.statistics"}""");
        BinderBehavior.Check(drained == Observe("""{"op":"managed.statistics"}"""), "post-unload statistics probes are repeatable");
        using (var statistics = JsonDocument.Parse(drained))
        {
            var data = statistics.RootElement.GetProperty("data");
            BinderBehavior.Check(!data.GetProperty("loaded").GetBoolean() && data.GetProperty("scripts").GetUInt64() == 0 && data.GetProperty("coroutines").GetUInt64() == 0 && data.GetProperty("bindings").GetUInt64() == 0, "post-unload statistics measure drained managed roots");
        }
        BinderBehavior.Check(NativeIdentity.Runtime is null && !witness.IsAlive, "unload drains managed identities and invalidates wrappers");
        NativeWorld.AttachNativeLibrary(nativePath, world);
        using var native = NativeWorld.Request("snapshot");
        BinderBehavior.Check(NativeWorld.Data(native).GetProperty("objects").GetArrayLength() == 1, "managed unload leaves native world lifetime to host");
        var nativeWitness = NativeWorld.Data(native).GetProperty("objects")[0];
        BinderBehavior.Check(nativeWitness.GetProperty("transform").GetProperty("position")[1].GetSingle() == 1, "unload disposes actual suspended enumerator before reporting zero coroutines");
        _ = NativeIdentity.GameObject(nativeWitness.GetProperty("handle").GetUInt64());
        using (var statistics = JsonDocument.Parse(Observe("""{"op":"managed.statistics"}""")))
            BinderBehavior.Check(!statistics.RootElement.GetProperty("data").GetProperty("loaded").GetBoolean() && statistics.RootElement.GetProperty("data").GetProperty("bindings").GetUInt64() > 0, "statistics reports actual identity cache entries even when fixture is unloaded");
        BinderBehavior.Check(unload() == 0, "repeated unload drains newly attached identity cache");
        BinderBehavior.Check(Call(load, payload.ToJsonString()) == 0, "reload uses fresh script roots");
        BinderBehavior.Check(unload() == 0 && unload() == 0, "repeated unload remains safe");
        Console.WriteLine("PASS Cdecl bridge: load failure/recovery, exact payload, private dispatch native effects, observation probes/no partial writes, reload and drained unload");
    }
    internal static int Call(delegate* unmanaged[Cdecl]<byte*, nuint, int> entry, string value)
    {
        var bytes = Encoding.UTF8.GetBytes(value);
        fixed (byte* p = bytes) return entry(p, (nuint)bytes.Length);
    }
    internal static string Observe(string query)
    {
        var bytes = Encoding.UTF8.GetBytes(query);
        delegate* unmanaged[Cdecl]<byte*, nuint, byte*, nuint, nuint*, int> entry = &ManagedBridge.Observe;
        fixed (byte* q = bytes)
        {
            nuint size = 0;
            BinderBehavior.Check(entry(q, (nuint)bytes.Length, null, 0, &size) == -2 && size > 0, "Observe length probe must request bounded buffer");
            var output = Enumerable.Repeat((byte)0xA5, (int)size).ToArray();
            fixed (byte* p = output)
            {
                BinderBehavior.Check(entry(q, (nuint)bytes.Length, p, size - 1, &size) == -2 && output.All(b => b == 0xA5), "Observe insufficient buffer writes no partial bytes");
                BinderBehavior.Check(entry(q, (nuint)bytes.Length, p, size, &size) == 0, "Observe copy succeeds");
            }
            return Encoding.UTF8.GetString(output);
        }
    }
    internal static string LastError()
    {
        delegate* unmanaged[Cdecl]<byte*, nuint, nuint*, int> entry = &ManagedBridge.GetLastError;
        nuint size = 0; entry(null, 0, &size);
        var bytes = new byte[(int)size];
        fixed (byte* p = bytes) entry(p, size, &size);
        return Encoding.UTF8.GetString(bytes);
    }
}
public class BridgeLifecycleProbe : BridgeProbe
{
    public static BridgeLifecycleProbe? Instance;
    public int CurrentX => (int)transform.position.x;
    private void Awake() => Instance = this;
    private System.Collections.IEnumerator Start()
    {
        try { yield return new WaitForSeconds(100); }
        finally { var p = transform.position; p.y++; transform.position = p; }
    }
}
public class BridgeProbe : MonoBehaviour
{
    private void Update() { var p = transform.position; p.x++; transform.position = p; }
}
