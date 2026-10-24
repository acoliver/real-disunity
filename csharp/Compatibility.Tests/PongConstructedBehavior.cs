using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;
using UnityEngine.UI;

internal static class PongConstructedBehavior
{
    public static void Run(string repository)
    {
        var ball = new GameObject("Ball") { tag = "Ball" };
        var body = ball.AddComponent<Rigidbody2D>(); ball.AddComponent<SpriteRenderer>(); ball.AddComponent<CircleCollider2D>();
        var goal = new GameObject("Goal"); goal.transform.position = new Vector3(2, 0);
        goal.AddComponent<BoxCollider2D>().isTrigger = true;
        var scores = new GameObject("Scores"); var manager = new GameObject("MatchManager");
        var view1 = new GameObject("Score1"); var text1 = view1.AddComponent<Text>();
        var view2 = new GameObject("Score2"); var text2 = view2.AddComponent<Text>();
        var player = new GameObject("Player"); player.transform.position = new Vector3(-4, 0); player.AddComponent<Rigidbody2D>().bodyType = RigidbodyType2D.Kinematic;
        var handles = new Dictionary<string, ulong>();
        var nodes = new[] { ball, goal, scores, manager, view1, view2, player };
        foreach (var node in nodes) handles.Add(node.name, node.NativeHandle);
        var specifications = new[]
        {
            Spec("ball-script", ball, "Ball", new { speed = 5f }),
            Spec("goal-script", goal, "Goal", new { scoresTo = 1, scoreEvent = new { m_PersistentCalls = new { m_Calls = new[] { new { m_Target = TestGraph.Ref("match-script"), m_MethodName = "ScorePoint", m_Mode = 0, m_CallState = 2, m_Arguments = new { } } } } } }),
            Spec("score-script", scores, "ScoreManager", new { }),
            Spec("score-view-manager", scores, "ScoreViewManager", new { currentScore = new[] { TestGraph.Ref("view1"), TestGraph.Ref("view2") } }),
            Spec("view1", view1, "ScoreView", new { }), Spec("view2", view2, "ScoreView", new { }),
            Spec("match-script", manager, "MatchManager", new { timeBeforeLaunch = 0.2f }),
            Spec("player-script", player, "Player", new { Speed = 5f, axis = "Vertical" })
        };
        foreach (var spec in specifications) handles.Add(spec.Id, BinderBehavior.AddScript(spec.Owner, spec.Type));
        using var payload = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            assemblies = new[] { Path.Combine(Environment.GetEnvironmentVariable("DISUNITY_TEST_MANAGED_ROOT") ?? Path.Combine(repository, ".work/managed"), "artifacts/bin/Pong/debug/Pong.dll") }, handles,
            graph = new
            {
                nodes = nodes.Select(n => new { id = n.name, template = false, active = true }), assets = Array.Empty<object>(),
                components = specifications.Select(s => new { id = s.Id, @object = s.Owner.name, script = new { type_name = s.Type, fields = s.Fields } })
            }, profile = new { id = "compat-2017.1" }
        }));
        using var session = new ScriptSession(payload.RootElement);
        CheckTypes(session);
        session.Initialize(); session.Dispatch("Start");
        BinderBehavior.Check(body.velocity.sqrMagnitude == 50, "original Pong BallMovement uses native random and speed");
        NativeWorld.Request("input.configure_axes", ("axes", new { Vertical = new { negative_keys = new[] { "S" }, positive_keys = new[] { "W" } } })).Dispose();
        NativeWorld.Request("input.set", ("keys", new[] { "W" })).Dispose();
        NativeWorld.Request("frame.begin", ("dt", 0.1f)).Dispose();
        NativeWorld.Request("time.phase", ("phase", "fixed")).Dispose();
        BinderBehavior.Check(MathF.Abs(Time.deltaTime - 0.02f) < 1e-6f && Input.GetAxisRaw("Vertical") == 1, "native phase clock and configured W axis reach original Pong input proxy");
        session.Dispatch("FixedUpdate"); NativeWorld.Request("physics.step", ("dt", 0.02f)).Dispose();
        NativeWorld.Request("time.phase", ("phase", "update")).Dispose();
        BinderBehavior.Check(MathF.Abs(Time.deltaTime - 0.1f) < 1e-6f, "native Update phase restores frame delta without C# clock");
        BinderBehavior.Check(MathF.Abs(player.transform.position.y - 0.1f) < 1e-5f, "original Pong Player FixedUpdate queues native MovePosition");
        body.velocity = new Vector2(5, 0);
        NativeWorld.Request("physics.step", ("dt", 0.4f)).Dispose();
        using var contacts = NativeWorld.Request("events.read");
        foreach (var contact in NativeWorld.Data(contacts).EnumerateArray()) session.InvokeContact(contact);
        BinderBehavior.Check(body.IsSleeping() && !ball.GetComponent<SpriteRenderer>()!.enabled, "original native trigger invokes serialized Goal -> MatchManager event, resets/hides/sleeps ball");
        BinderBehavior.Check(text1.text == "0" && text2.text == "1", "original ScoreManager/ScoreViewManager/ScoreView update native Text");
        BinderBehavior.Check(session.CoroutineCount == 1, "original Pong relaunch enumerator is scheduled");
        TestGraph.Advance(session, 0.1f);
        BinderBehavior.Check(!ball.GetComponent<SpriteRenderer>()!.enabled, "Pong relaunch waits on native time");
        TestGraph.Advance(session, 0.1f);
        BinderBehavior.Check(ball.GetComponent<SpriteRenderer>()!.enabled && body.velocity.sqrMagnitude == 50 && !body.IsSleeping(), "original coroutine relaunch resumes/wakes ball");
        BinderBehavior.Check(session.CoroutineCount == 0, "completed original Pong enumerator is drained");
        Console.WriteLine("PASS original Pong constructed-native-world: 20 types, native Player input/MovePosition, native trigger/serialized event/score Text, reset and original timed relaunch");
    }
    private static ScriptSpec Spec(string id, GameObject owner, string type, object fields) => new(id, owner, type, fields);
    private static void CheckTypes(ScriptSession session)
    {
        var names = new[] { "Ball", "BallMovement", "Tags", "Goal", "IInput", "IRandom", "IScoreEvent", "IScoreViewManager", "ITime", "MatchManager", "Player", "PlayerMovement", "InputProxy", "RandomProxy", "TimeProxy", "Players", "ScoreManager", "ScoreView", "ScoreViewManager", "RandomUtils" };
        foreach (var type in names) BinderBehavior.Check(session.ResolveType(type) is not null, "actual Pong type missing: " + type);
    }
}
