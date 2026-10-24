using System.Reflection;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class OriginalGameBehavior
{
    private static void Check(bool condition, string message) => BinderBehavior.Check(condition, message);
    public static void Snake(string repository, string? profile = null)
    {
        using var payload = GraphFixture.Payload(repository, "Snake", "snake-ready", profile);
        using var session = new ScriptSession(payload.RootElement);
        foreach (var type in new[] { "GameLoop", "GameConfig", "SnakeModel", "SnakeView", "GridSystem", "InputBuffer", "PlayerInputReader", "FoodSpawner" }) Check(session.ResolveType(type) is not null, "actual Snake type missing: " + type);
        session.Initialize();
        Check(session.ScriptCount == 4, "Snake must bind all four scene scripts, with config separate");
        session.Dispatch("Start");
        var food = PublicObservationBehavior.Data(session, """{"op":"managed.public","type":"FoodSpawner","path":"CurrentFoodPos"}""");
        var spawner = UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None).Single(s => s.GetType().Name == "FoodSpawner");
        var visual = (Transform)spawner.GetType().GetField("foodVisual", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(spawner)!;
        Check(visual.position == new Vector3(food.GetProperty("x").GetInt32(), food.GetProperty("y").GetInt32()), "actual FoodSpawner public getter matches original SpawnNext native visual");
        var grid = PublicObservationBehavior.Data(session, """{"op":"managed.query","type":"GameConfig","member":"gridSize"}""");
        Check(grid.GetProperty("x").GetInt32() == 40 && grid.GetProperty("y").GetInt32() == 20, "public query includes actual bound ScriptableObject authoring");
        NativeWorld.Request("input.set", ("keys", Array.Empty<string>()), ("axes", new Dictionary<string, float>())).Dispose();
        NativeWorld.Request("frame.begin", ("dt", 0.125f)).Dispose();
        session.Dispatch("Update");
        var view = UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None).Single(s => s.GetType().Name == "SnakeView");
        Check(view.transform.childCount == 1, "actual SnakeView must instantiate native segment template");
        var segment = view.transform.GetChild(0);
        Check(segment.position == new Vector3(21, 10), "actual GameLoop must use imported config (40x20) and move SnakeModel");
        Check(segment.gameObject.activeInHierarchy, "cloned segment must be activated natively");
        var authoredSprite = segment.GetComponent<SpriteRenderer>()!.sprite!;
        Check(authoredSprite.rect.width == 8 && authoredSprite.pixelsPerUnit == 8, "explicit importer fallback sprite retains authored geometry");
        segment.GetComponent<SpriteRenderer>()!.sprite = authoredSprite;
        Check(ReferenceEquals(segment.GetComponent<SpriteRenderer>()!.sprite, authoredSprite), "native sprite writes retain cached asset identity");
        NativeWorld.Request("input.set", ("keys", new[] { "W" }), ("axes", new Dictionary<string, float>())).Dispose();
        NativeWorld.Request("frame.begin", ("dt", 0.125f)).Dispose();
        session.Dispatch("Update");
        Check(segment.position == new Vector3(21, 11), "original PlayerInputReader must steer native segment on W");
        Check(ReferenceEquals(view, view.GetComponent<MonoBehaviour>()), "actual MonoBehaviour GetComponent cache identity");
        session.Dispose();
        Check(session.ScriptCount == 0 && session.CoroutineCount == 0 && session.BindingCount == 0 && NativeIdentity.BindingCount == 0, "actual Snake teardown drains scripts, bindings and identity cache");
        Console.WriteLine("PASS original Snake: eight types, graph/config/prefab references, Start, native movement and input steering");
    }
    public static void Breakout(string repository)
    {
        using var payload = GraphFixture.Payload(repository, "Breakout", "breakout-ready");
        using var session = new ScriptSession(payload.RootElement);
        foreach (var type in new[] { "Ball", "BallSpawner", "Brick", "BrickSpawner", "DeathZone", "Platform", "PowerUp" }) Check(session.ResolveType(type) is not null, "actual Breakout type missing: " + type);
        session.Initialize();
        var authoredPlatform = UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None).Single(s => s.GetType().Name == "Platform");
        var initialPlatformPosition = authoredPlatform.transform.position;
        AdvanceBreakout(session, ["D"]);
        Check(authoredPlatform.transform.position.x > initialPlatformPosition.x, "actual first worker fixed phase starts scene and moves original platform through native physics");
        var scripts = UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        var bricks = scripts.Where(s => s.GetType().Name == "Brick").ToArray();
        Check(bricks.Length == 48, "original BrickSpawner must create 6x8 native bricks with sprite geometry");
        AdvanceBreakout(session, []);
        Check(UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None).Count(s => s.GetType().Name == "Brick") == 48,
            "repeated worker fixed/update/late phases must preserve exactly 48 bricks without restarting active spawner");
        var ball = scripts.Single(s => s.GetType().Name == "Ball");
        Check(ball.GetComponent<Rigidbody2D>()!.linearVelocity.magnitude > 0, "instantiated Ball Start must launch before fixed update");
        var platform = scripts.Single(s => s.GetType().Name == "Platform");
        var col = platform.GetComponent<BoxCollider2D>()!;
        var originalSize = col.size;
        platform.GetType().GetMethod("MakeWider")!.Invoke(platform, [0.1f]);
        Check(col.size.x > originalSize.x, "actual WiderRoutine must run its original enumerator immediately");
        NativeWorld.Request("frame.begin", ("dt", 0.05f)).Dispose(); session.Dispatch("Update");
        Check(col.size.x > originalSize.x, "WaitForSeconds must not resume early");
        NativeWorld.Request("frame.begin", ("dt", 0.05f)).Dispose(); session.Dispatch("Update");
        Check(col.size == originalSize, "actual platform coroutine must restore native collider after native time");
        var brick = bricks[0];
        using var contact = Collision(ball, brick, false);
        session.InvokeContact(contact.RootElement);
        Check(!brick.GetComponent<Collider2D>()!.enabled, "actual BreakRoutine disables native collider before wait");
        for (var i = 0; i < 12 && brick.IsAlive; i++) { NativeWorld.Request("frame.begin", ("dt", 0.05f)).Dispose(); session.Dispatch("Update"); }
        Check(brick == null, "actual original Brick animation coroutine must finish and destroy native object");
        session.Dispose();
        Check(session.ScriptCount == 0 && session.CoroutineCount == 0 && session.BindingCount == 0 && NativeIdentity.BindingCount == 0, "actual Breakout teardown clears static singleton roots and clone bindings");
        Console.WriteLine("PASS original Breakout: seven types, 48 native clones, sprite arrays, private callbacks, original animation and width coroutines");
    }
    private static void AdvanceBreakout(ScriptSession session, string[] keys)
    {
        NativeWorld.Request("input.set", ("keys", keys), ("axes", new Dictionary<string, float>())).Dispose();
        NativeWorld.Request("frame.begin", ("dt", 0.02f)).Dispose();
        NativeWorld.Request("time.phase", ("phase", "fixed")).Dispose();
        session.Dispatch("FixedUpdate");
        NativeWorld.Request("physics.step", ("dt", 0.02f)).Dispose();
        using var events = NativeWorld.Request("events.read");
        foreach (var contact in NativeWorld.Data(events).EnumerateArray()) session.InvokeContact(contact);
        NativeWorld.Request("time.phase", ("phase", "update")).Dispose();
        session.Dispatch("Update");
        session.Dispatch("LateUpdate");
    }
    public static void Pong(string repository, string? profile = null)
    {
        using var payload = GraphFixture.Payload(repository, "Pong", "pong-ready", profile);
        using var session = new ScriptSession(payload.RootElement);
        var names = new[] { "Ball", "BallMovement", "Tags", "Goal", "IInput", "IRandom", "IScoreEvent", "IScoreViewManager", "ITime", "MatchManager", "Player", "PlayerMovement", "InputProxy", "RandomProxy", "TimeProxy", "Players", "ScoreManager", "ScoreView", "ScoreViewManager", "RandomUtils" };
        foreach (var type in names) Check(session.ResolveType(type) is not null, "actual Pong type missing: " + type);
        session.Initialize(); session.Dispatch("Start");
        var scripts = UObject.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        var ball = scripts.Single(s => s.GetType().Name == "Ball");
        var body = ball.GetComponent<Rigidbody2D>()!;
        Check(body.velocity.sqrMagnitude > 0, "original Pong Ball must launch");
        var goal = scripts.First(s => s.GetType().Name == "Goal");
        using var contact = Collision(goal, ball, true);
        session.InvokeContact(contact.RootElement);
        Check(body.IsSleeping() && !ball.GetComponent<SpriteRenderer>()!.enabled, "original Goal UnityEvent must reset/hide/sleep ball through MatchManager");
        var texts = UObject.FindObjectsByType<UnityEngine.UI.Text>(FindObjectsSortMode.None);
        Check(texts.Count(t => t.text == "1") == 1, "original serialized dynamic UnityEvent must increment native UI score once");
        var manager = scripts.Single(s => s.GetType().Name == "MatchManager");
        var delay = (float)manager.GetType().GetField("timeBeforeLaunch")!.GetValue(manager)!;
        Check(session.CoroutineCount == 1, "actual Pong relaunch enumerator is scheduled");
        var steps = (int)Math.Ceiling(delay / 0.25f) + 1;
        for (var i = 0; i < steps; i++) { NativeWorld.Request("frame.begin", ("dt", 0.25f)).Dispose(); session.Dispatch("Update"); }
        Check(ball.GetComponent<SpriteRenderer>()!.enabled && body.velocity.sqrMagnitude > 0 && !body.IsSleeping(), "native time must resume original relaunch and wake ball");
        Check(session.CoroutineCount == 0, "completed original enumerator must be released");
        session.Dispose();
        Check(session.ScriptCount == 0 && session.CoroutineCount == 0 && session.BindingCount == 0 && NativeIdentity.BindingCount == 0, "actual Pong teardown drains scripts, event bindings and identity cache");
        Console.WriteLine("PASS original Pong: twenty types, serialized score events/list/UI, body reset, native-clock relaunch coroutine");
    }
    private static JsonDocument Collision(MonoBehaviour a, MonoBehaviour b, bool trigger) => JsonDocument.Parse(JsonSerializer.Serialize(new
    {
        phase = "enter", trigger, a = a.gameObject.NativeHandle, b = b.gameObject.NativeHandle,
        collider_a = a.GetComponent<Collider2D>()!.NativeHandle, collider_b = b.GetComponent<Collider2D>()!.NativeHandle,
        normal = new[] { 1f, 0f }, point = new[] { 0f, 0f }
    }));
}
