using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class ContactBehavior
{
    public static void Run()
    {
        var a = new GameObject("contact-a"); var b = new GameObject("contact-b");
        var ca = a.AddComponent<BoxCollider2D>(); var cb = b.AddComponent<BoxCollider2D>();
        var body = a.AddComponent<Rigidbody2D>(); body.bodyType = RigidbodyType2D.Dynamic;
        a.transform.position = new Vector3(-2, 0); b.transform.position = new Vector3(0, 0);
        body.velocity = new Vector2(10, 0);
        using var payload = TestGraph.Payload([new("a-script", a, "ContactProbe", new { }), new("b-script", b, "ContactProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement); session.Initialize();
        NativeWorld.Request("physics.step", ("dt", 0.2f)).Dispose();
        using var response = NativeWorld.Request("events.read");
        var events = NativeWorld.Data(response).EnumerateArray().Where(e => e.GetProperty("phase").GetString() == "enter").ToArray();
        BinderBehavior.Check(events.Length == 1, "native swept collision must produce one enter");
        var contact = events[0]; session.InvokeContact(contact);
        BinderBehavior.Check(a.transform.localScale.x == 1 && b.transform.localScale.x == -1, "native contact dispatch must reach both owners with opposite normals");
        BinderBehavior.Check(a.transform.localScale.y == 2 && b.transform.localScale.y == 2, "private contact callbacks use actual cached collider arguments");
        var before = JsonSerializer.Serialize(session.Observe(JsonDocument.Parse("{}").RootElement));
        var after = JsonSerializer.Serialize(session.Observe(JsonDocument.Parse("{}").RootElement));
        BinderBehavior.Check(before == after, "observation must not dispatch or mutate callbacks");
        Console.WriteLine("PASS contacts: native swept event, both owners, reversed normals, private callback and read-only observation");
    }
}
public class ContactProbe : MonoBehaviour
{
    private void OnCollisionEnter2D(Collision2D collision)
    {
        if (!ReferenceEquals(collision.collider, collision.gameObject.GetComponent<Collider2D>())) throw new Exception("contact identity mismatch");
        var scale = transform.localScale;
        scale.x = collision.GetContact(0).normal.x; scale.y++;
        transform.localScale = scale;
    }
}
