using System.Collections;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static class CallbackStartBehavior
{
    public static void Run()
    {
        var first = new GameObject("iterator-start-failure");
        var peer = new GameObject("iterator-start-peer");
        using var graph = TestGraph.Payload([
            new("first", first, "ThrowingStartProbe", new { }),
            new("peer", peer, "CallbackIsolationProbe", new { })]);
        using var session = new ScriptSession(graph.RootElement);
        session.Dispatch("Start");
        BinderBehavior.Check(peer.transform.position.z == 1, "iterator Start failure must continue eligible peer Start");
        using var query = JsonDocument.Parse("""{"op":"managed.failures"}""");
        using var response = JsonDocument.Parse(JsonSerializer.Serialize(session.Observe(query.RootElement)));
        var data = response.RootElement.GetProperty("data");
        BinderBehavior.Check(data.GetProperty("total").GetInt64() == 1 &&
            data.GetProperty("records")[0].GetProperty("message").GetString() == "iterator Start sentinel", "iterator Start exception remains observable");
        session.Dispatch("Start");
        BinderBehavior.Check(peer.transform.position.z == 1, "iterator Start failure never retries Start");
        Console.WriteLine("PASS callback iterator Start: initial MoveNext isolation, eligible peer continuation, once-only Start and original exception");
    }
}
public class ThrowingStartProbe : MonoBehaviour
{
    private IEnumerator Start()
    {
        yield return Fail();
    }
    private static object Fail() => throw new ArgumentException("iterator Start sentinel");
}
