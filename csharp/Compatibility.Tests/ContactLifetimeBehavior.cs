using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class ContactLifetimeBehavior
{
    private static (GameObject Zone, GameObject Ball) Scene()
    {
        var zone = new GameObject("lifetime-zone");
        zone.AddComponent<BoxCollider2D>().isTrigger = true;
        var ball = new GameObject("lifetime-ball");
        ball.AddComponent<CircleCollider2D>();
        ball.AddComponent<Rigidbody2D>().gravityScale = 0;
        return (zone, ball);
    }

    private static JsonElement[] Step()
    {
        NativeWorld.Request("physics.step", ("dt", 0.02f)).Dispose();
        using var response = NativeWorld.Request("events.read");
        return NativeWorld.Data(response).EnumerateArray().Select(e => e.Clone()).ToArray();
    }

    public static void QueuedExit()
    {
        var (zone, ball) = Scene();
        var oldCollider = ball.GetComponent<Collider2D>()!.NativeHandle;
        using var payload = TestGraph.Payload([new("destroyer", zone, "ContactDestroyProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        var enters = Step();
        BinderBehavior.Check(enters.Any(e => e.GetProperty("phase").GetString() == "enter"), "native overlap must generate the destruction callback");
        foreach (var contact in enters) session.InvokeContact(contact);
        BinderBehavior.Check(!ball.IsAlive && zone.transform.localScale.x == 2, "native enter callback must destroy the ball exactly once");
        var replacement = new GameObject("replacement-ball");
        replacement.transform.position = new Vector3(20, 20);
        var replacementCollider = replacement.AddComponent<CircleCollider2D>();
        BinderBehavior.Check(replacement.NativeHandle != ball.NativeHandle && replacementCollider.NativeHandle != oldCollider, "replacement identities must have fresh native generations");
        var exits = Step();
        BinderBehavior.Check(exits.Any(e => e.GetProperty("phase").GetString() == "exit" &&
            (e.GetProperty("collider_a").GetUInt64() == oldCollider || e.GetProperty("collider_b").GetUInt64() == oldCollider)), "native physics must emit the queued exit for the destroyed collider");
        foreach (var contact in exits) session.InvokeContact(contact);
        BinderBehavior.Check(zone.transform.localScale.y == 1 && replacementCollider.enabled, "invalidated native exit must not reach scripts or a replacement collider generation");
        Console.WriteLine("PASS contact lifetime: native callback destruction, queued exit and replacement generation");
    }

    public static void RemainingRecipients() => RemainingRecipients(false);
    public static void ColliderRemoval() => RemainingRecipients(true);

    private static void RemainingRecipients(bool removeCollider)
    {
        var (zone, ball) = Scene();
        var collider = ball.GetComponent<Collider2D>()!;
        using var payload = TestGraph.Payload([
            new("destroyer", zone, "ContactDestroyProbe", new { removeCollider }),
            new("witness", zone, "ContactRemainingProbe", new { }),
            new("ball-witness", ball, "ContactRemainingProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        foreach (var contact in Step()) session.InvokeContact(contact);
        BinderBehavior.Check(!collider.IsAlive && ball.IsAlive == removeCollider && zone.transform.localScale.y == 1,
            "callbacks after native participant destruction must not receive the invalidated event");
        if (removeCollider) BinderBehavior.Check(ball.transform.localScale.y == 1, "surviving opposite owner must not receive a removed collider event");
        Console.WriteLine($"PASS contact lifetime: native {(removeCollider ? "collider removal" : "object destruction")} cancels remaining recipients");
    }

    public static void CallbackFailure()
    {
        var (zone, _) = Scene();
        using var payload = TestGraph.Payload([new("failure", zone, "ContactFailureProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        var enter = Step().Single(e => e.GetProperty("phase").GetString() == "enter");
        string expected;
        try
        {
            NativeWorld.Request("contact.unsupported").Dispose();
            throw new Exception("unsupported native operation was accepted");
        }
        catch (NativeWorldException error) { expected = error.Message; }
        session.InvokeContact(enter);
        using var query = JsonDocument.Parse("""{"op":"managed.failures"}""");
        using var observation = JsonDocument.Parse(JsonSerializer.Serialize(session.Observe(query.RootElement)));
        var data = observation.RootElement.GetProperty("data");
        BinderBehavior.Check(data.GetProperty("total").GetInt64() == 1, "unknown native callback error remains a failure");
        var failure = data.GetProperty("records")[0];
        BinderBehavior.Check(failure.GetProperty("native_code").GetString() == "invalid_request" &&
            failure.GetProperty("exception_type").GetString() == typeof(NativeWorldException).FullName &&
            failure.GetProperty("message").GetString() == expected &&
            failure.GetProperty("stack").GetString()!.Contains("ContactFailureProbe.OnTriggerEnter2D"),
            "unknown native callback error must preserve type/code/message/stack unchanged");
        Console.WriteLine("PASS contact lifetime: unrelated native callback failures remain visible as structured evidence");
    }
}

public class ContactDestroyProbe : MonoBehaviour
{
    public bool removeCollider = false;
    private void OnTriggerEnter2D(Collider2D other)
    {
        var scale = transform.localScale;
        scale.x++;
        transform.localScale = scale;
        UObject.Destroy(removeCollider ? other : other.gameObject);
    }
    private void OnTriggerExit2D(Collider2D other)
    {
        var scale = transform.localScale;
        scale.y++;
        transform.localScale = scale;
        _ = other.enabled;
    }
}

public class ContactRemainingProbe : MonoBehaviour
{
    private void OnTriggerEnter2D(Collider2D other)
    {
        var scale = transform.localScale;
        scale.y++;
        transform.localScale = scale;
        _ = other.enabled;
    }
}

public class ContactFailureProbe : MonoBehaviour
{
    private void OnTriggerEnter2D(Collider2D other) => NativeWorld.Request("contact.unsupported").Dispose();
}
