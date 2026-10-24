using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static class NestedOriginalBehavior
{
    internal static void Breakout(string path, string repository, bool authoredGrid = false)
    {
        using var world = new ReadinessWorld(path, nestedSession: true);
        using var payload = GraphFixture.Payload(repository, "Breakout", "breakout-ready");
        var manifest = JsonNode.Parse(payload.RootElement.GetRawText())!;
        var scripts = new List<ScriptSpec> { new("driver", new GameObject("original-nested-driver"), nameof(NestedBreakoutDriver), new { authoredGrid }) };
        if (authoredGrid)
        {
            var spawner = manifest["graph"]!["components"]!.AsArray().Single(c => c?["script"]?["type_name"]?.GetValue<string>() == "BrickSpawner")!;
            var handle = manifest["handles"]![spawner["id"]!.GetValue<string>()]!.GetValue<ulong>();
            NativeWorld.Request("component.set", ("component", handle), ("enabled", false)).Dispose();
            for (var i = 0; i < 48; i++)
            {
                var brick = new GameObject("authored-brick-" + i);
                brick.transform.position = new Vector3((i % 8) * 2 - 7, 4 + i / 8, 0);
                brick.AddComponent<SpriteRenderer>(); brick.AddComponent<BoxCollider2D>();
                scripts.Add(new("authored-brick-" + i, brick, "Brick", new { frameTime = 0.05f }));
            }
        }
        var driver = world.Manifest(scripts.ToArray());
        foreach (var key in new[] { "nodes", "components" })
            foreach (var node in driver["graph"]![key]!.AsArray()) manifest["graph"]![key]!.AsArray().Add(node!.DeepClone());
        foreach (var handle in driver["handles"]!.AsObject()) manifest["handles"]![handle.Key] = handle.Value!.DeepClone();
        manifest["assemblies"]!.AsArray().Add(typeof(NestedOriginalBehavior).Assembly.Location);
        manifest["requirements"] = driver["requirements"]!.DeepClone(); manifest["world"] = world.World;
        BinderBehavior.Check(world.Load(manifest) == 0, "original nested Breakout load: " + BridgeBehavior.LastError());
        var session = (ScriptSession)NativeIdentity.Runtime!;
        var control = session.Resolve("driver");
        int Value(string field) => (int)control.GetType().GetField(field)!.GetValue(control)!;
        for (var i = 0; i < 300 && Value("widened") == 0; i++) world.Frame(1, 50);
        BinderBehavior.Check(Value("grid") == 48, "original fixture has sufficient 6x8 grid; actual=" + Value("grid") + "; failures=" + BridgeBehavior.Observe("{\"op\":\"managed.failures\"}"));
        BinderBehavior.Check(Value("widened") == 1 && Value("broken") > 0, "original Brick.BreakRoutine -> PowerUp.Apply -> Platform.MakeWider -> WiderRoutine executes on native contacts");
        BinderBehavior.Check(session.CallbackFailureCount == 0, "original nested path has no callback failures");
        for (var i = 0; i < 20; i++) world.Frame(1, 50);
        BinderBehavior.Check(Value("restored") == 1, "original WiderRoutine resumes at native deadline and restores collider");
        world.Cleanup();
        Console.WriteLine("PASS original Breakout nested path: unchanged seven-source PE, authored 48-brick grid, native physical contacts, actual power-up widening and deadline restoration");
    }
}
public sealed class NestedBreakoutDriver : MonoBehaviour
{
    public int grid, broken, widened, restored;
    public bool authoredGrid;
    private MonoBehaviour? _target;
    private int _initial;
    private void Start()
    {
        var scripts = UnityEngine.Object.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        if (authoredGrid)
        {
            var spawner = scripts.Single(s => s.GetType().Name == "BrickSpawner");
            var types = (Array)spawner.GetType().GetField("brickTypes")!.GetValue(spawner)!;
            var data = types.GetValue(0)!;
            var sprite = data.GetType().GetField("initialSprite")!.GetValue(data);
            var frames = data.GetType().GetField("breakFrames")!.GetValue(data);
            foreach (var brick in scripts.Where(s => s.GetType().Name == "Brick")) brick.GetType().GetMethod("Setup")!.Invoke(brick, [sprite, frames]);
        }
        var power = scripts.Single(s => s.GetType().Name == "PowerUp");
        power.GetType().GetField("chance")!.SetValue(power, 1f);
        power.GetType().GetField("widePlatformDuration")!.SetValue(power, 0.125f);
    }
    private void Update()
    {
        var scripts = UnityEngine.Object.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None);
        var bricks = scripts.Where(s => s.GetType().Name == "Brick").ToArray();
        if (_initial == 0) { grid = bricks.Length; _initial = grid; }
        broken = _initial - bricks.Length;
        var platform = scripts.Single(s => s.GetType().Name == "Platform");
        var normal = (Vector2)platform.GetType().GetField("normalColliderSize")!.GetValue(platform)!;
        var size = platform.GetComponent<BoxCollider2D>()!.size;
        if (size.x > normal.x) widened = 1;
        if (widened != 0) { if (size == normal) restored = 1; return; }
        if (_target == null) _target = bricks.FirstOrDefault();
        var ball = scripts.First(s => s.GetType().Name == "Ball");
        if (_target != null)
        {
            ball.transform.position = _target.transform.position;
            ball.GetComponent<Rigidbody2D>()!.linearVelocity = Vector2.zero;
        }
    }
}
