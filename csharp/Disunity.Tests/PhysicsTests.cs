using Disunity.Hosting;
using UnityEngine;

internal static class PhysicsTests
{
    private sealed class Probe : MonoBehaviour
    {
        public readonly List<string> Calls = new();
        public Collider2D? Other;
        private void FixedUpdate() => Calls.Add("fixed");
        private void Update() => Calls.Add("update");
        private void OnCollisionEnter2D(Collision2D collision) { Other = collision.collider; Calls.Add("enter"); }
        private void OnCollisionStay2D(Collision2D collision) => Calls.Add("stay");
        private void OnCollisionExit2D(Collision2D collision) => Calls.Add("exit");
        private void OnTriggerEnter2D(Collider2D collider) { Other = collider; Calls.Add("trigger-enter"); }
        private void OnTriggerStay2D(Collider2D collider) => Calls.Add("trigger-stay");
        private void OnTriggerExit2D(Collider2D collider) => Calls.Add("trigger-exit");
    }
    public static void Run()
    {
        RunContacts(false);
        RunContacts(true);
        Console.WriteLine("PASS: native collision/trigger order, identity and physics queries");
    }
    private static void RunContacts(bool trigger)
    {
        DisunityHost.LoadWorld();
        var a = DisunityHost.AddComponent(new Probe());
        var body = a.gameObject.AddComponent<Rigidbody2D>();
        body.gravityScale = 0;
        try { body.mass = 0; throw new Exception("invalid mass accepted"); }
        catch (DisunityException) { }
        Require(body.mass == 1, "invalid body mutation is atomic");
        var ca = a.gameObject.AddComponent<BoxCollider2D>();
        ca.isTrigger = trigger;
        var b = DisunityHost.AddComponent(new Probe());
        var cb = b.gameObject.AddComponent<BoxCollider2D>();
        b.transform.position = new Vector3(0.8f, 0, 0);
        var prefix = trigger ? "trigger-" : "";
        DisunityHost.Step(0.02f);
        Require(a.Calls.SequenceEqual(new[] { "fixed", prefix + "enter", "update" }), "fixed/contact/update order");
        Require(ReferenceEquals(a.Other, cb) && ReferenceEquals(b.Other, ca), "one wrapper per contact identity");
        a.Calls.Clear();
        DisunityHost.Step(0.02f);
        Require(a.Calls.Contains(prefix + "stay"), "contact stay");
        Require(Physics2D.OverlapCircle(new Vector2(0.8f, 0), 0.1f) == cb, "native overlap circle");
        Require(Physics2D.Raycast(new Vector2(-3, 0), new Vector2(1, 0), 10).collider == ca, "native raycast");
        b.transform.position = new Vector3(5, 0, 0);
        a.Calls.Clear();
        DisunityHost.Step(0.02f);
        Require(a.Calls.Contains(prefix + "exit"), "contact exit");
        body.velocity = new Vector2(2, 0);
        var before = a.transform.position.X;
        DisunityHost.Step(0.02f);
        Require(a.transform.position.X > before, "native body velocity moves authoritative transform");
        var clone = UnityEngine.Object.Instantiate(a.gameObject);
        Require(clone.GetComponent<Rigidbody2D>()?.velocity == body.velocity, "cloned native body wrapper");
        Require(clone.GetComponent<BoxCollider2D>()?.isTrigger == trigger, "cloned native collider wrapper");
        clone.transform.position = new Vector3(20, 0, 0);
        DisunityHost.Step(0.02f);
        DisunityHost.UnloadWorld();
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
