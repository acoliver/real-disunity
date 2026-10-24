using System.Text;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static class LifecycleReadBudgetBehavior
{
    public static void Run()
    {
        var owner = new GameObject("lifecycle-read-budget");
        var inactive = new GameObject("lifecycle-inactive-without-coroutines");
        inactive.SetActive(false);
        using var payload = TestGraph.Payload(Enumerable.Range(0, 16)
            .Select(i => new ScriptSpec("probe-" + i, owner, nameof(LifecycleBudgetProbe), new { }))
            .Append(new ScriptSpec("inactive", inactive, nameof(LifecycleBudgetProbe), new { })).ToArray());
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        session.Dispatch("Start");
        var native = NativeWorld.Current;
        var measured = new MeasuredWorld(native);
        NativeWorld.Attach(measured);
        try
        {
            session.Dispatch("LateUpdate");
            BinderBehavior.Check(measured.Queries == 2,
                $"mutation-free phase must use two fresh eligibility reads; actual {measured.Queries}");
            BinderBehavior.Check(measured.Bytes <= 2 * (64 + 48 * 17),
                $"script-only replies exceeded per-script payload budget: {measured.Bytes}");
            measured.Reset();
            session.Dispatch("Update");
            BinderBehavior.Check(measured.Queries == 18,
                $"each of 16 real callbacks must refresh native eligibility plus two phase reads; actual {measured.Queries}");
            using var query = JsonDocument.Parse("{\"op\":\"managed.snapshot\"}");
            var result = JsonSerializer.SerializeToElement(session.Observe(query.RootElement));
            BinderBehavior.Check(result.GetProperty("data").GetProperty("instances").EnumerateArray()
                .All(s => s.GetProperty("fields").GetProperty("Calls").GetInt32() == (s.GetProperty("awake").GetBoolean() ? 1 : 0)),
                "all eligible real callbacks must execute exactly once");
        }
        finally { NativeWorld.Attach(native); }
        Console.WriteLine("PASS lifecycle read budget: two fresh phase reads, one fresh read per real callback, bounded script-only bytes");
    }

    private sealed class MeasuredWorld(INativeWorld native) : INativeWorld
    {
        public int Queries { get; private set; }
        public int Bytes { get; private set; }
        public void Reset() { Queries = 0; Bytes = 0; }
        public JsonDocument Request(JsonDocument request)
        {
            var response = native.Request(request);
            if (request.RootElement.GetProperty("op").GetString() is "lifecycle.snapshot" or "lifecycle.scripts")
            {
                Queries++;
                Bytes += Encoding.UTF8.GetByteCount(response.RootElement.GetRawText());
            }
            return response;
        }
        public double TimeValue(string field) => native.TimeValue(field);
    }
}

public sealed class LifecycleBudgetProbe : MonoBehaviour
{
    public int Calls;
    private void Update() => Calls++;
}
