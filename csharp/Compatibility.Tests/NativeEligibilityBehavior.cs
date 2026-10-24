using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class NativeEligibilityBehavior
{
    public static void Liveness()
    {
        var owner = new GameObject("native-liveness");
        var component = owner.AddComponent<BoxCollider2D>();
        var native = NativeWorld.Current;
        var reads = new ObservedWorld(native);
        NativeWorld.Attach(reads);
        try
        {
            BinderBehavior.Check(owner.IsAlive && component.IsAlive, "native object and component must initially be alive");
            BinderBehavior.Check(!reads.Operations.Contains("object.get") && !reads.Operations.Contains("component.get"),
                "liveness resource contract must avoid full object/component serialization");
            NativeWorld.Request("component.remove", ("component", component.NativeHandle)).Dispose();
            BinderBehavior.Check(!component.IsAlive && owner.IsAlive, "removed component must die without retiring its owner");
            NativeWorld.Request("object.destroy", ("object", owner.NativeHandle)).Dispose();
            var replacement = new GameObject("native-replacement");
            BinderBehavior.Check(!owner.IsAlive && replacement.IsAlive && owner.NativeHandle != replacement.NativeHandle,
                "native slot reuse must preserve retired generational liveness");
            var asset = ScriptableObject.CreateInstance<ScriptableObject>();
            NativeIdentity.MarkData(asset, "native-independent-data");
            BinderBehavior.Check(asset.IsAlive && !new UObject().IsAlive, "data assets and zero handles retain their liveness rules");
            NativeIdentity.Reset();
            BinderBehavior.Check(!asset.IsAlive && !replacement.IsAlive, "epoch reset invalidates both data assets and native wrappers");
        }
        finally { NativeWorld.Attach(native); }
        Console.WriteLine("PASS native liveness: compact reads, component retirement, generation reuse, data assets and epoch guards");
    }

    public static void Lifecycle()
    {
        var owner = new GameObject("compact-lifecycle");
        using var payload = TestGraph.Payload([new("probe", owner, "LifecycleMutationProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement);
        var native = NativeWorld.Current;
        var reads = new ObservedWorld(native);
        NativeWorld.Attach(reads);
        try
        {
            session.Initialize();
            session.Dispatch("Start");
            BinderBehavior.Check(owner.transform.position == Vector3.one, "compact eligibility must deliver Awake, OnEnable and Start");
            BinderBehavior.Check(!reads.Operations.Contains("snapshot") && reads.Operations.Contains("lifecycle.scripts"),
                "lifecycle resource contract must avoid full world serialization");
            BinderBehavior.Check(!reads.Operations.Contains("object.get") && !reads.Operations.Contains("component.get"),
                "lifecycle eligibility must avoid full per-script object/component serialization");
        }
        finally { NativeWorld.Attach(native); }
        Console.WriteLine("PASS lifecycle resource contract: native eligibility without full world snapshots");
    }

    private sealed class ObservedWorld(INativeWorld native) : INativeWorld
    {
        public HashSet<string> Operations { get; } = new(StringComparer.Ordinal);
        public JsonDocument Request(JsonDocument request)
        {
            Operations.Add(request.RootElement.GetProperty("op").GetString()!);
            return native.Request(request);
        }
        public double TimeValue(string field) => native.TimeValue(field);
    }
}
