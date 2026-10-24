using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;

internal static class CompoundContactBehavior
{
    public static void Run()
    {
        var parent = new GameObject("compound-body"); var child = new GameObject("compound-collider");
        child.transform.SetParent(parent.transform, false);
        parent.transform.position = new Vector3(-2, 0);
        var body = parent.AddComponent<Rigidbody2D>(); body.velocity = new Vector2(10, 0);
        var collider = child.AddComponent<BoxCollider2D>(); collider.isTrigger = true;
        var other = new GameObject("compound-other"); other.AddComponent<BoxCollider2D>();
        using var payload = TestGraph.Payload([
            new("parent", parent, "TriggerProbe", new { }), new("child", child, "TriggerProbe", new { }), new("other", other, "TriggerProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement); session.Initialize();
        NativeWorld.Request("physics.step", ("dt", 0.2f)).Dispose();
        using var result = NativeWorld.Request("events.read");
        foreach (var contact in NativeWorld.Data(result).EnumerateArray()) session.InvokeContact(contact);
        BinderBehavior.Check(parent.transform.position.z == 1 && child.transform.localPosition.z == 1 && other.transform.position.z == 1,
            "compound native trigger reaches collider owner, ancestor body owner and other owner exactly once");
        BinderBehavior.Check(ReferenceEquals(collider.attachedRigidbody, body), "collider attachedRigidbody must return native ancestor body instance");
        Console.WriteLine("PASS compound contacts: native collider/body recipients, duplicate suppression and ancestor body identity");
    }
}
public class TriggerProbe : MonoBehaviour
{
    private void OnTriggerEnter2D(Collider2D other)
    {
        if (!ReferenceEquals(other, other.GetComponent<Collider2D>())) throw new Exception("other collider identity mismatch");
        var p = transform.localPosition; p.z++; transform.localPosition = p;
    }
}
