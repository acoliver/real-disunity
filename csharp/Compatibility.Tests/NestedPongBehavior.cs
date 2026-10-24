using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static class NestedPongBehavior
{
    private static void ConfigureAxes() => NativeWorld.Request("input.configure_axes", ("axes", new
    {
        Vertical = new { negative_keys = new[] { "S" }, positive_keys = new[] { "W" } },
        AlternateVertical = new { negative_keys = new[] { "DownArrow" }, positive_keys = new[] { "UpArrow" } }
    })).Dispose();
    internal static void Run(string path, string repository, bool nestedSession)
    {
        using var world = new ReadinessWorld(path, nestedSession: nestedSession);
        ConfigureAxes();
        using var payload = GraphFixture.Payload(repository, "Pong", "pong-ready");
        var manifest = JsonNode.Parse(payload.RootElement.GetRawText())!;
        var driver = world.Manifest([new("driver", new GameObject("original-pong-contact-driver"), nameof(NestedPongDriver), new { })]);
        foreach (var key in new[] { "nodes", "components" })
            foreach (var node in driver["graph"]![key]!.AsArray()) manifest["graph"]![key]!.AsArray().Add(node!.DeepClone());
        foreach (var handle in driver["handles"]!.AsObject()) manifest["handles"]![handle.Key] = handle.Value!.DeepClone();
        manifest["assemblies"]!.AsArray().Add(typeof(NestedPongBehavior).Assembly.Location);
        manifest["requirements"] = driver["requirements"]!.DeepClone(); manifest["world"] = world.World;
        BinderBehavior.Check(world.Load(manifest) == 0, "original Pong fixture load: " + BridgeBehavior.LastError());
        var session = (ScriptSession)NativeIdentity.Runtime!;
        world.Frame(1, 50);
        var scripts = UnityEngine.Object.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        var ball = scripts.Single(s => s.GetType().Name == "Ball");
        var manager = scripts.Single(s => s.GetType().Name == "MatchManager");
        BinderBehavior.Check(ball.GetComponent<SpriteRenderer>()!.enabled, "original Ball.Start initially shows ball");
        world.Frame(1, 50);
        BinderBehavior.Check(!ball.GetComponent<SpriteRenderer>()!.enabled && ball.GetComponent<Rigidbody2D>()!.IsSleeping(), "real Goal contact invokes original ScorePoint and suspended LaunchBall");
        var delay = (float)manager.GetType().GetField("timeBeforeLaunch")!.GetValue(manager)!;
        for (var i = 0; i < (int)Math.Ceiling(delay * 50) + 2; i++) world.Frame(1, 50);
        BinderBehavior.Check(ball.GetComponent<SpriteRenderer>()!.enabled && ball.GetComponent<Rigidbody2D>()!.velocity.sqrMagnitude > 0, "original LaunchBall resumes on native deadline");
        BinderBehavior.Check(session.CallbackFailureCount == 0, "original Pong Start/contact path has no application failure; " + BridgeBehavior.Observe("{\"op\":\"managed.failures\"}"));
        world.Cleanup();
        Console.WriteLine($"PASS original Pong characterization nested={nestedSession}: original Ball.Start and Goal contact -> MatchManager.ScorePoint -> LaunchBall; no MatchManager.Start exists in this source");
    }
}
public sealed class NestedPongDriver : MonoBehaviour
{
    private bool _placed;
    private void Update()
    {
        if (_placed) return;
        _placed = true;
        var scripts = UnityEngine.Object.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        var ball = scripts.Single(s => s.GetType().Name == "Ball");
        var goal = scripts.First(s => s.GetType().Name == "Goal");
        ball.transform.position = goal.transform.position;
        ball.GetComponent<Rigidbody2D>()!.velocity = Vector2.zero;
    }
}
