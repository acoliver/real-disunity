#define CONTRACTS_FULL
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static class PublicObservationBehavior
{
    public static void Run()
    {
        var first = new GameObject("public-first");
        var second = new GameObject("public-second");
        using var payload = TestGraph.Payload([new("first", first, "PublicObservationProbe", new { amount = 17 }), new("second", second, "PublicObservationProbe", new { amount = 23 })]);
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        var handle = session.Resolve("first").NativeHandle;
        CheckFailure(session, """{"op":"managed.query","type":"PublicObservationProbe","member":"Amount"}""", "ambiguous");
        var before = NativeSnapshot();
        BinderBehavior.Check(Data(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"GetAmount"}""").GetInt32() == 17, "explicit declared pure method getter returns actual state");
        var query = JsonSerializer.Serialize(new { op = "managed.query", type = "PublicObservationProbe", member = "Amount", handle });
        BinderBehavior.Check(Data(session, query).GetInt32() == 17 && Data(session, query).GetInt32() == 17, "public getter returns actual deserialized value repeatedly");
        BinderBehavior.Check(Data(session, """{"op":"managed.public","type":"PublicObservationProbe","id":"second","path":"amount"}""").GetInt32() == 23, "public fields and stable identity disambiguation");
        BinderBehavior.Check(Data(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","path":"Position.x"}""").GetInt32() == 17, "public property and nested value field traversal");
        CheckFailure(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"Mutate"}""", "read-only");
        CheckFailure(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"secret"}""", "public");
        CheckFailure(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"SetterOnly"}""", "public");
        var cycle = Data(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"Cycle"}""");
        BinderBehavior.Check(cycle.GetProperty("next").GetProperty("cycle").GetBoolean(), "cyclic data is bounded explicitly");
        var items = Data(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"Items","max_items":2}""");
        BinderBehavior.Check(items.GetArrayLength() == 2 && items[1].GetInt32() == 2, "enumeration observes actual prefix within item budget");
        CheckFailure(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"Large"}""", "budget");
        CheckFailure(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"ManyNodes"}""", "budget");
        CheckFailure(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"Items","max_items":0}""", "max_items");
        var shallow = Data(session, """{"op":"managed.query","type":"PublicObservationProbe","id":"first","member":"Nested","max_depth":1}""");
        BinderBehavior.Check(shallow.GetProperty("next").GetProperty("truncated").GetBoolean(), "nested DTO depth is bounded explicitly");
        BinderBehavior.Check(NativeSnapshot() == before, "public observation and rejected mutators leave native world unchanged");
        session.Dispose();
        try { Data(session, query); throw new Exception("disposed query accepted"); }
        catch (ObjectDisposedException) { }
        Console.WriteLine("PASS public observations: real getter/field values, nested paths, stable/handle ambiguity, rejected private/mutator access, cycle/item/byte budgets and unchanged native world");
    }
    internal static JsonElement Data(ScriptSession session, string query)
    {
        using var request = JsonDocument.Parse(query);
        using var response = JsonDocument.Parse(JsonSerializer.Serialize(session.Observe(request.RootElement)));
        return response.RootElement.GetProperty("data").Clone();
    }
    private static void CheckFailure(ScriptSession session, string query, string diagnostic)
    {
        try { Data(session, query); throw new Exception("query unexpectedly accepted: " + query); }
        catch (Exception error) when (!error.Message.StartsWith("query unexpectedly", StringComparison.Ordinal))
        {
            BinderBehavior.Check(error.Message.Contains(diagnostic, StringComparison.OrdinalIgnoreCase), "query diagnostic: " + error.Message);
        }
    }
    private static string NativeSnapshot() { using var value = NativeWorld.Request("snapshot"); return value.RootElement.GetRawText(); }
}
public class PublicObservationProbe : MonoBehaviour
{
    public int amount;
    private int secret = 99;
    public int Amount => amount;
    [System.Diagnostics.Contracts.Pure] public int GetAmount() => amount;
    public PublicObservationNode Nested => new() { next = new() };
    public int[][] ManyNodes => Enumerable.Repeat(Enumerable.Range(0, 256).ToArray(), 256).ToArray();
    public Vector2Int Position => new(amount, secret);
    public int SetterOnly { private get; set; }
    public PublicObservationNode Cycle { get; } = new();
    public int[] Items => [1, 2, 3, 4];
    public string Large => new('x', 70000);
    public void Mutate() => transform.position = new Vector3(999, 999);
}
public sealed class PublicObservationNode
{
    public PublicObservationNode next;
    public PublicObservationNode() { next = this; }
}
