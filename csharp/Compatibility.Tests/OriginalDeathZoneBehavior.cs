using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class OriginalDeathZoneBehavior
{
    public static void Run(string repository)
    {
        using var payload = GraphFixture.Payload(repository, "Breakout", "breakout-ready");
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        session.Dispatch("Start");
        var scripts = UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        var zone = scripts.Single(s => s.GetType().Name == "DeathZone");
        var spawner = scripts.Single(s => s.GetType().Name == "BallSpawner");
        var prefab = (GameObject)spawner.GetType().GetField("ballPrefab")!.GetValue(spawner)!;
        var first = Balls().Single();
        var secondObject = UObject.Instantiate(prefab, new Vector3(3, 0), Quaternion.identity);
        session.Dispatch("Start");
        var second = Balls().Single(b => b.gameObject.NativeHandle == secondObject.NativeHandle);
        BinderBehavior.Check(Balls().Length == 2 && first.GetType() == second.GetType(), "two original Ball instances must come from the source prefab");
        Lose(session, zone, first, second, final: false);
        Lose(session, zone, second, null, final: true);
        var replacement = Balls().Single();
        BinderBehavior.Check(replacement.NativeHandle != first.NativeHandle && replacement.NativeHandle != second.NativeHandle,
            "last-ball loss must create a fresh native Ball generation");
        session.Dispatch("FixedUpdate");
        BinderBehavior.Check(replacement.GetComponent<Rigidbody2D>()!.linearVelocity.y > 0 && replacement.GetComponent<SpriteRenderer>()!.enabled,
            "original Ball Start must launch the replacement upward with its source sprite enabled");
        session.Dispose();
        BinderBehavior.Check(session.ScriptCount == 0 && session.CoroutineCount == 0 && session.BindingCount == 0 && NativeIdentity.BindingCount == 0,
            "original two-ball loss teardown must release all managed roots");
        Console.WriteLine("PASS original DeathZone: native physical two-ball loss preserves pending count, final loss respawns one upward original Ball");
    }

    private static MonoBehaviour[] Balls() => UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None).Where(s => s.GetType().Name == "Ball").ToArray();

    private static void Lose(ScriptSession session, MonoBehaviour zone, MonoBehaviour ball, MonoBehaviour? survivor, bool final)
    {
        // Position the source ball above the authored trigger, then let real native physics enter it.
        var collider = zone.GetComponent<BoxCollider2D>()!;
        var center = zone.transform.position + (Vector3)collider.offset;
        ball.transform.position = center + new Vector3(3, collider.size.y * 0.5f + 0.6f);
        ball.GetComponent<Rigidbody2D>()!.linearVelocity = new Vector2(0, -6);
        var found = false;
        NativeWorld.Request("lifetime.begin").Dispose();
        try
        {
            for (var i = 0; i < 8 && !found; i++)
            {
                NativeWorld.Request("physics.step", ("dt", 0.02f)).Dispose();
                using var events = NativeWorld.Request("events.read");
                foreach (var contact in NativeWorld.Data(events).EnumerateArray())
                {
                    if (IsLoss(contact, zone.gameObject.NativeHandle, ball.gameObject.NativeHandle)) found = true;
                    session.InvokeContact(contact);
                }
            }
            BinderBehavior.Check(found, "real native physics must generate original DeathZone trigger enter");
            var pending = Balls();
            BinderBehavior.Check(ball.IsAlive && pending.Length == 2, final
                ? "final pending original ball and its one source-spawned replacement must both remain visible until end"
                : "first pending original ball and survivor must remain visible without premature replacement");
            if (survivor is not null) BinderBehavior.Check(pending.Any(b => b.NativeHandle == survivor.NativeHandle), "first loss preserves the other original ball");
        }
        finally { NativeWorld.Request("lifetime.end").Dispose(); }
        session.Reconcile();
        var remaining = Balls();
        BinderBehavior.Check(!ball.IsAlive && remaining.Length == 1, "native lifetime end must retire only the pending ball");
        if (survivor is not null) BinderBehavior.Check(remaining[0].NativeHandle == survivor.NativeHandle, "two-ball loss must not spawn a replacement");
    }

    private static bool IsLoss(JsonElement contact, ulong zone, ulong ball)
    {
        var a = contact.GetProperty("a").GetUInt64();
        var b = contact.GetProperty("b").GetUInt64();
        return contact.GetProperty("trigger").GetBoolean() && contact.GetProperty("phase").GetString() == "enter"
            && ((a == zone && b == ball) || (a == ball && b == zone));
    }
}
