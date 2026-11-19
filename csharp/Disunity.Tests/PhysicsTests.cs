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
        RunInterpolationModes();
        RunImportedInterpolationModes();
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
        var before = a.transform.position.x;
        DisunityHost.Step(0.02f);
        Require(a.transform.position.x > before, "native body velocity moves authoritative transform");
        var clone = UnityEngine.Object.Instantiate(a.gameObject);
        Require(clone.GetComponent<Rigidbody2D>()?.velocity == body.velocity, "cloned native body wrapper");
        Require(clone.GetComponent<BoxCollider2D>()?.isTrigger == trigger, "cloned native collider wrapper");
        clone.transform.position = new Vector3(20, 0, 0);
        DisunityHost.Step(0.02f);
        DisunityHost.UnloadWorld();
    }
    public static void RunInterpolationModes()
    {
        DisunityHost.LoadWorld();
        try
        {
            var body = DisunityHost.CreateGameObject().AddComponent<Rigidbody2D>();
            Require(body.interpolation == RigidbodyInterpolation2D.None, "default presentation mode");
            body.interpolation = RigidbodyInterpolation2D.Extrapolate;
            Require(body.interpolation == RigidbodyInterpolation2D.Extrapolate, "extrapolation enum round-trip");
            body.velocity = new Vector2(10, 0);
            body.gravityScale = 0;
            Require(body.interpolation == RigidbodyInterpolation2D.Extrapolate, "body writes preserve extrapolation");
            var clone = UnityEngine.Object.Instantiate(body.gameObject).GetComponent<Rigidbody2D>()!;
            Require(clone.interpolation == RigidbodyInterpolation2D.Extrapolate, "cloning preserves native extrapolation");
            foreach (var invalid in new[] { (RigidbodyInterpolation2D)3, (RigidbodyInterpolation2D)(-1) })
            {
                try { body.interpolation = invalid; throw new InvalidOperationException("invalid interpolation accepted"); }
                catch (DisunityException) { }
                Require(body.interpolation == RigidbodyInterpolation2D.Extrapolate, "invalid mode does not mutate the body");
            }
            DisunityHost.Step(0.03f);
            Require(MathF.Abs(body.position.x - 0.2f) < 0.000001f, "scripts keep the fixed-step physics pose");
            foreach (var mode in Enum.GetValues<RigidbodyInterpolation2D>())
            {
                body.interpolation = mode;
                Require(body.interpolation == mode, "all presentation modes round-trip");
            }
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: native Rigidbody2D presentation modes, clone and invalid-tag atomicity");
    }

    public static void RunImportedInterpolationModes()
    {
        foreach (var mode in Enum.GetValues<RigidbodyInterpolation2D>())
        {
            DisunityHost.LoadWorld();
            try
            {
                var plan = """
                    {"scene":"physics","sprites":[],"textures":[],"fonts":[],"axes":[],"assets":[],"differences":[],
                     "objects":[{"name":"body","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,
                     "position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,
                     "components":[{"type":"Rigidbody2D","body_type":"Dynamic","mass":1,"gravity_scale":0,
                     "constraints":0,"material":null,"simulated":true,"interpolation":"MODE"}]}]}
                    """.Replace("MODE", mode.ToString());
                using var scene = DisunityHost.LoadScene(plan, new SceneLoadOptions(typeof(PhysicsTests).Assembly, _ => []));
                Require(scene.Differences.Count == 0, "supported imported presentation mode has no difference");
                Require(scene.Objects[0].GetComponent<Rigidbody2D>()!.interpolation == mode, "imported interpolation enum round-trip");
            }
            finally { DisunityHost.UnloadWorld(); }
        }
        Console.WriteLine("PASS: imported Rigidbody2D None, Interpolate and Extrapolate modes");
    }

    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
