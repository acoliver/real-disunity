using Disunity.Hosting;
using UnityEngine;
using Object = UnityEngine.Object;

internal static class DeferredDestroyTests
{
    private sealed class Ball : MonoBehaviour
    {
        public int Updates;
        public int Disabled;
        public int DestroyedCount;
        public Action? Cleanup;
        private void Update() => Updates++;
        private void OnDisable() => Disabled++;
        private void OnDestroy() { DestroyedCount++; Cleanup?.Invoke(); }
    }
    private sealed class Observer : MonoBehaviour
    {
        public Action? Tick;
        public Action? Late;
        private void Update() => Tick?.Invoke();
        private void LateUpdate() => Late?.Invoke();
    }
    private sealed class Goal : MonoBehaviour
    {
        public int Count;
        public int Contacts;
        private void OnTriggerEnter2D(Collider2D other)
        {
            Contacts++;
            Object.Destroy(other.gameObject);
            Object.Destroy(other.gameObject);
            Count = Object.FindObjectsByType<Ball>(FindObjectsSortMode.None).Length;
            Require(Count == 2, "pending ball remains visible in trigger callback");
            Require(other.gameObject != null && other, "pending equality stays non-null");
            other.transform.position = new(0, 0, 0);
        }
    }
    public static void Run()
    {
        VisibilityAndNestedCleanup();
        TriggerCountsBothBalls();
        BothContactsKeepPendingBallsVisible();
        Constraints();
        DelayedDestruction();
        Console.WriteLine("PASS: pending Destroy searches, callbacks, nested cleanup, position constraints and interpolation");
    }
    private static void VisibilityAndNestedCleanup()
    {
        DisunityHost.LoadWorld();
        var first = DisunityHost.AddComponent(new Ball());
        var second = DisunityHost.AddComponent(new Ball());
        first.gameObject.tag = "Ball";
        first.Cleanup = () => {
            Require(first && first.transform.position == new Vector3(2, 3, 0), "OnDestroy keeps native handle valid");
            first.transform.position = new(4, 5, 0);
            Require(Object.FindObjectsByType<Ball>(FindObjectsSortMode.None).Length == 2, "cleanup searches retain pending scripts");
            Object.Destroy(first.gameObject);
            Object.Destroy(second.gameObject);
        };
        var observations = 0;
        Action observe = () => {
            Require(first && Object.FindObjectsByType<Ball>(FindObjectsSortMode.None).Length == 2,
                "pending objects stay visible through Update and LateUpdate");
            observations++;
        };
        var observer = DisunityHost.AddComponent(new Observer { Tick = observe, Late = observe });
        Object.Destroy(first.gameObject);
        Object.Destroy(first.gameObject);
        Require(first && first.gameObject && first.Disabled == 0, "cleanup waits for frame barrier");
        Require(ReferenceEquals(Object.FindFirstObjectByType<Ball>(), first), "first lookup sees pending");
        Require(ReferenceEquals(Object.FindObjectOfType<Ball>(), first), "legacy lookup sees pending");
        Require(ReferenceEquals(GameObject.FindGameObjectWithTag("Ball"), first.gameObject), "tag lookup sees pending");
        first.transform.position = new(2, 3, 0);
        Require(first.transform.position == new Vector3(2, 3, 0), "pending writes remain valid");
        DisunityHost.Step(0);
        observer.Tick = null;
        observer.Late = null;
        Require(observations == 2, "both dynamic callbacks precede destruction barrier");
        Require(!first && !second && first == null && second == null, "nested cleanup retires both before completion");
        Require(first!.Updates == 0 && first.Disabled == 1 && first.DestroyedCount == 1, "pending callbacks suppressed and cleanup once");
        Require(second!.Disabled == 1 && second.DestroyedCount == 1, "nested cleanup once");
        Require(Object.FindObjectsByType<Ball>(FindObjectsSortMode.None).Length == 0, "search excludes retired objects");
        Require(GameObject.FindGameObjectWithTag("Ball") == null, "tag lookup excludes retired object");
        try { first.transform.position = Vector3.one; throw new Exception("retired write accepted"); }
        catch (ObjectDisposedException) { }
        DisunityHost.Step(0);
        Require(first.Updates == 0 && first.DestroyedCount == 1, "next frame never dispatches retired scripts");
        DisunityHost.UnloadWorld();
    }
    private static void TriggerCountsBothBalls()
    {
        DisunityHost.LoadWorld();
        var goal = DisunityHost.AddComponent(new Goal());
        goal.gameObject.AddComponent<BoxCollider2D>().isTrigger = true;
        var first = DisunityHost.AddComponent(new Ball());
        first.gameObject.AddComponent<CircleCollider2D>();
        var body = first.gameObject.AddComponent<Rigidbody2D>();
        body.gravityScale = 0;
        body.velocity = new(10, 0);
        first.transform.position = new(-1, 0, 0);
        var second = DisunityHost.AddComponent(new Ball());
        DisunityHost.Step(0.02f);
        Require(goal.Contacts == 1 && goal.Count == 2, "two-ball goal does not request a replacement");
        Require(!first && second && first.DestroyedCount == 1, "only struck ball retires");
        DisunityHost.Step(0.02f);
        Require(first.Updates == 0, "destroyed ball receives no next-frame Update");
        DisunityHost.UnloadWorld();
    }
    private static void BothContactsKeepPendingBallsVisible()
    {
        DisunityHost.LoadWorld();
        var goal = DisunityHost.AddComponent(new Goal());
        goal.gameObject.AddComponent<BoxCollider2D>().isTrigger = true;
        var balls = new[] { DisunityHost.AddComponent(new Ball()), DisunityHost.AddComponent(new Ball()) };
        foreach (var ball in balls)
        {
            ball.gameObject.AddComponent<CircleCollider2D>().isTrigger = true;
            var body = ball.gameObject.AddComponent<Rigidbody2D>();
            body.gravityScale = 0;
            body.velocity = new(10, 0);
            ball.transform.position = new(-1, 0, 0);
        }
        DisunityHost.Step(0.02f);
        Require(goal.Contacts == 2 && goal.Count == 2, "separate contacts both see two pending balls");
        Require(balls.All(ball => !ball && ball.Disabled == 1 && ball.DestroyedCount == 1), "both balls retire once");
        DisunityHost.UnloadWorld();
    }
    private static void Constraints()
    {
        DisunityHost.LoadWorld();
        var body = DisunityHost.CreateGameObject().AddComponent<Rigidbody2D>();
        body.constraints = RigidbodyConstraints2D.FreezePositionX | RigidbodyConstraints2D.FreezeRotation;
        Require(body.freezeRotation && (int)body.constraints == 5, "constraints ABI round trip");
        body.freezeRotation = false;
        Require(body.constraints == RigidbodyConstraints2D.FreezePositionX, "rotation property preserves position mask");
        body.MovePosition(new(100, 2));
        DisunityHost.Step(0.02f);
        Require(body.position.x == 0 && MathF.Abs(body.position.y - 2) < 0.0001f, "managed constrained MovePosition");
        body.constraints = RigidbodyConstraints2D.FreezeAll;
        body.velocity = new(100, 100);
        DisunityHost.Step(0.02f);
        Require(body.position.x == 0 && MathF.Abs(body.position.y - 2) < 0.0001f, "FreezeAll locks position");
        Require(body.interpolation == RigidbodyInterpolation2D.None, "bodies start without interpolation");
        body.interpolation = RigidbodyInterpolation2D.Interpolate;
        body.mass = 2;
        Require(body.interpolation == RigidbodyInterpolation2D.Interpolate, "interpolation survives other body writes");
        try { body.interpolation = RigidbodyInterpolation2D.Extrapolate; throw new Exception("extrapolation accepted"); }
        catch (NotSupportedException) { }
        Require(body.interpolation == RigidbodyInterpolation2D.Interpolate, "refused extrapolation keeps the setting");
        DisunityHost.UnloadWorld();
    }
    private static void DelayedDestruction()
    {
        DisunityHost.LoadWorld();
        var ball = DisunityHost.AddComponent(new Ball());
        Object.Destroy(ball.gameObject, 0.05f);
        Object.Destroy(ball.gameObject, 1f);
        DisunityHost.Step(0.02f);
        Require(ball && ball.Updates == 1, "delayed object keeps ordinary callbacks before deadline");
        DisunityHost.Step(0.04f);
        Require(!ball && ball.Updates == 1 && ball.DestroyedCount == 1, "earliest scaled delay wins");
        DisunityHost.UnloadWorld();
    }
    private static void Require(bool value, string message)
    {
        if (!value) throw new InvalidOperationException(message);
    }
}
