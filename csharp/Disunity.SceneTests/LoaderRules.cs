using System.Reflection;
using System.Text.Json;
using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Events;
using static Check;

public enum Mood { Calm, Angry }

[Serializable]
public sealed class Wave : ISerializationCallbackReceiver
{
    public int count = 1;
    public Sprite? art;
    [NonSerialized] public string? loaded;
    public void OnBeforeSerialize() { }
    public void OnAfterDeserialize() => loaded = $"count={count}";
}

public sealed class Settings : ScriptableObject
{
    public float gravity = 1;
}

public static class Outer
{
    public sealed class Inner : MonoBehaviour { public int value; }
}

public sealed class Rules : MonoBehaviour, ISerializationCallbackReceiver
{
    public int count = 5;
    public float speed;
    public bool on;
    public Mood mood;
    public string label = "x";
    public Vector2 at;
    public Vector3 far;
    public Vector2Int cell;
    public Color tint;
    public LayerMask mask;
    public RenderingLayerMask renderingMask;
    [SerializeField] private GameObject? other = null;
    public Transform? anchor;
    public Rules? peer;
    public Animator? animator;
    public Camera? view;
    public Wave[] waves = [];
    public List<Wave> more = new();
    public Settings? settings;
    public UnityEvent<int> changed = new();
    public Dictionary<string, int>? table;
    public int kept = 9;
    [NonSerialized] public int skipped = 3;
    public int received;
    public char letter;
    public ulong big;
    public sbyte small;
    public float low;
    public string? awake;
    [NonSerialized] public string? deserialized;
    public GameObject? Other => other;
    public void OnBeforeSerialize() { }
    public void OnAfterDeserialize() => deserialized = $"count={count} waves={waves.Length} loaded={(waves.Length > 0 ? waves[0].loaded : null)} awake={awake ?? "no"}";
    public void Receive(int value) => received = value;
    private void Awake() => awake = $"{label} peer={peer?.gameObject.name} kept={kept}";
}

/// <summary>Unity's field binding rules and the loader's failures, on synthetic plans whose
/// scripts are this assembly's.</summary>
internal static class LoaderRules
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            FieldsBindAsUnityDeserializesThem();
            AxesRegisterOnlyWhenTheEngineHasTheirKeys();
            FailuresNameTheFieldAndLeaveNothingBehind();
            PlayerPlansNameScriptsByAssembly();
            ValueFieldRules.Check();
            ReferencesToUnbuiltComponents();
            Console.WriteLine("PASS: scene plans bind fields, value types, events, assets and axes by Unity's rules and fail cleanly");
        }
        finally
        {
            DisunityHost.UnloadWorld();
        }
    }

    internal static SceneLoadOptions Options => new(Assembly.GetExecutingAssembly(), _ => SyntheticFont.Bytes());

    private const string Fields = """
        {"count": "7", "speed": "2.5", "on": "1", "mood": "1", "label": "hi",
         "at": {"x": "1", "y": "2"}, "far": {"x": "1", "y": "2", "z": "3"}, "cell": {"x": "4", "y": "5"},
         "tint": {"r": "1", "g": "0", "b": "0", "a": "1"}, "mask": {"m_Bits": "256"}, "renderingMask": {"m_Bits": "5"},
         "other": {"$object": 1}, "anchor": {"$transform": 1}, "peer": {"$component": [1, 0]},
         "waves": [{"count": "2", "art": {"$sprite": 0}}], "more": [{"count": "3", "art": null}],
         "settings": {"$asset": 0}, "skipped": "8", "removedField": "1",
         "letter": "233", "big": "18446744073709551615", "small": "-2", "low": "-Infinity",
         "changed": {"m_PersistentCalls": {"m_Calls": [{"m_Target": {"$component": [1, 0]}, "m_MethodName": "Receive",
           "m_Mode": "0", "m_CallState": "2", "m_Arguments": {"m_ObjectArgument": null, "m_IntArgument": "0",
           "m_FloatArgument": "0", "m_StringArgument": "", "m_BoolArgument": "0"}}]}}}
        """;

    /// <summary>A plan of object A with a Rules script given <paramref name="fields"/>, and its
    /// child B with a default Rules script.</summary>
    internal static string Plan(string fields, string axes = "[]", string script = "Rules", string? unplanned = null)
    {
        JsonNode Script(string fields) => JsonNode.Parse(
            """{"type": "Script", "script": "Assets/Rules.cs", "class": """ + $"\"{script}\"" + """, "enabled": true, "fields": """ + fields + "}")!;
        static JsonNode Object(string name, int? parent, JsonNode script, string? extra = null) => new JsonObject
        {
            ["name"] = name, ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = parent,
            ["template"] = false, ["position"] = new JsonArray(0, 0, 0), ["rotation"] = new JsonArray(0, 0, 0, 1),
            ["scale"] = new JsonArray(1, 1, 1), ["rect"] = null,
            ["components"] = extra is null ? new JsonArray(script) : new JsonArray(script, JsonNode.Parse(extra)),
        };
        var plan = JsonNode.Parse("""{"scene": "Assets/Rules.unity", "fonts": [], "axes": """ + axes + """
            ,
             "assets": [{"type": "Script", "script": "Assets/Settings.cs", "class": "Settings", "name": "Heavy", "fields": {"gravity": "4"}}],
             "textures": [{"guid": "0", "path": "Assets/art.png", "width": 8, "height": 8, "filter": "Point", "wrap": ["Clamp", "Clamp"], "srgb": true}],
             "sprites": [{"texture": 0, "name": "art", "rect": [0, 0, 8, 8], "pivot": [0.5, 0.5], "pixels_per_unit": 8, "border": [0, 0, 0, 0], "physics_shape": null}]}
            """)!;
        plan["objects"] = new JsonArray(Object("A", null, Script(fields), unplanned), Object("B", 0, Script("{}")));
        return plan.ToJsonString();
    }

    private static void FieldsBindAsUnityDeserializesThem()
    {
        using var scene = DisunityHost.LoadScene(Plan(Fields), Options);
        var (a, b) = (scene.Objects[0].GetComponent<Rules>()!, scene.Objects[1].GetComponent<Rules>()!);
        Require((a.count, a.speed, a.on, a.mood, a.label) == (7, 2.5f, true, Mood.Angry, "hi"), "scalars and enums bind");
        Require((a.letter, a.big, a.small, a.low) == ('\u00e9', ulong.MaxValue, (sbyte)-2, float.NegativeInfinity),
            "chars, the full ulong range, negative narrow integers and infinities bind");
        Require(a.at == new Vector2(1, 2) && a.far == new Vector3(1, 2, 3) && a.cell == new Vector2Int(4, 5), "vectors bind");
        Require(a.tint == new Color(1, 0, 0, 1) && a.mask.value == 256, "colors and layer masks bind");
        Require(a.renderingMask.value == 5 && b.renderingMask.value == 0, "rendering layer masks bind from their m_Bits");
        Require(ReferenceEquals(a.Other, b.gameObject) && ReferenceEquals(a.anchor, b.transform) && ReferenceEquals(a.peer, b),
            "[SerializeField] fields and object, transform and component references bind");
        Require(a.waves is [{ count: 2, art: not null }] && a.more is [{ count: 3, art: null }], "serializable arrays and lists bind");
        Require(a.waves[0].art!.name == "art", "a planned sprite carries the name the plan gives it");
        Require(a.settings is { name: "Heavy", gravity: 4 }, "asset references bind to the loaded asset with its fields");
        Require(a.kept == 9 && a.skipped == 3, "absent fields keep initializers and [NonSerialized] fields ignore data");
        Require(a.awake == "hi peer=B kept=9", "fields bind before Awake");
        Require(a.deserialized == "count=7 waves=1 loaded=count=2 awake=no" && a.more[0].loaded == "count=3",
            $"OnAfterDeserialize runs after the fields, nested objects first, before Awake: {a.deserialized}");
        a.changed.Invoke(42);
        Require(b.received == 42, "persistent UnityEvent calls reach their target with the event's argument");
        a.changed.RemoveAllListeners();
        a.changed.Invoke(7);
        Require(b.received == 7 && a.changed.GetPersistentEventCount() == 1, "RemoveAllListeners keeps persistent calls, as in Unity");
    }

    private static void AxesRegisterOnlyWhenTheEngineHasTheirKeys()
    {
        const string axes = """
            [{"name": "Jump", "kind": "KeyOrMouseButton", "negative": "", "positive": "space", "alt_negative": "", "alt_positive": "",
              "gravity": 1000, "dead": 0.001, "sensitivity": 1000, "snap": false, "invert": false},
             {"name": "Fire", "kind": "KeyOrMouseButton", "negative": "", "positive": "left ctrl", "alt_negative": "", "alt_positive": "mouse 0",
              "gravity": 1000, "dead": 0.001, "sensitivity": 1000, "snap": false, "invert": false},
             {"name": "Mouse X", "kind": "MouseMovement", "negative": "", "positive": "", "alt_negative": "", "alt_positive": "",
              "gravity": 0, "dead": 0, "sensitivity": 0.1, "snap": false, "invert": false}]
            """;
        using var scene = DisunityHost.LoadScene(Plan("{}", axes), Options);
        Require(scene.UnregisteredAxes.SequenceEqual(["Fire: no key for `left ctrl`, `mouse 0`",
            "Mouse X: MouseMovement axes read a device the engine has no input for"]), string.Join("; ", scene.UnregisteredAxes));
        Key(KeyCode.Space, true);
        Steps(1);
        Require(Input.GetAxisRaw("Jump") == 1, "Jump reads space");
        Key(KeyCode.Space, false);
        Steps(1);
        Expect(() => Input.GetAxisRaw("Fire"), "not registered");
    }

    private static void FailuresNameTheFieldAndLeaveNothingBehind()
    {
        Fails(Plan("""{"peer": {"$object": 1}}"""), "A.Rules.peer: a Rules field references a GameObject");
        Fails(Plan("""{"table": {}}"""), "A.Rules.table: System.Collections.Generic.Dictionary");
        Fails(Plan(Fields.Replace("\"Receive\"", "\"Nope\"")), "A.Rules.changed call 0: Rules has no method Nope(Int32)");
        Fails(Plan("{}").Replace("\"class\":\"Rules\",\"enabled\":true,\"fields\":{}", "\"class\":\"Missing\",\"enabled\":true,\"fields\":{}"),
            "the game assembly has 0 MonoBehaviour classes named Missing");
    }

    /// <summary>A player's plan names each script's assembly and full class name, and keeps
    /// components it could not plan as differences.</summary>
    private static void PlayerPlansNameScriptsByAssembly()
    {
        var self = Assembly.GetExecutingAssembly();
        var name = self.GetName().Name!;
        string Player(string assembly, string fullName) => Plan("{}").Replace(
            "\"script\":\"Assets/Rules.cs\",\"class\":\"Rules\",\"enabled\":true,\"fields\":{}",
            $"\"script\":\"\",\"class\":\"{fullName}\",\"assembly\":\"{assembly}\",\"enabled\":true,\"fields\":{{\"value\":\"4\"}}}},"
            + "{\"type\":\"Unsupported\",\"class\":\"Light\"");
        var options = Options with { Assemblies = requested => requested == name ? self : throw new FileNotFoundException(requested) };
        using (var scene = DisunityHost.LoadScene(Player(name, "Outer/Inner"), options))
        {
            Require(scene.Objects.All(o => o.GetComponent<Outer.Inner>() is { value: 4 }), "a nested class resolves by assembly and full name");
            Require(scene.Differences.SequenceEqual(["A component 1 (Unsupported): Light components are not built yet",
                "B component 1 (Unsupported): Light components are not built yet"]), string.Join("; ", scene.Differences));
        }
        Fails(Player(name, "Outer/Missing"), $"{name} has no MonoBehaviour class Outer/Missing", options);
        Fails(Player(name, "Wave"), $"{name} has no MonoBehaviour class Wave", options);
        Fails(Player(name, "Outer/Inner"), $"the plan names assembly {name}, and the loader has no assembly resolver");
    }

    /// <summary>A player plan's Animator is not built, so a script's reference to it binds null
    /// and is listed; references to other unbuilt components, or into fields that cannot hold the
    /// class, still fail.</summary>
    private static void ReferencesToUnbuiltComponents()
    {
        const string animator = """{"type": "Unsupported", "class": "Animator"}""";
        const string reference = """{"animator": {"$component": [0, 1]}}""";
        using (var scene = DisunityHost.LoadScene(Plan(reference, unplanned: animator), Options))
        {
            Require(scene.Objects[0].GetComponent<Rules>()!.animator is null, "a reference to an unbuilt component binds null");
            Require(scene.Differences.SequenceEqual(["A component 1 (Unsupported): Animator components are not built yet",
                "A.Rules.animator: references a Animator component, which is not built yet"]), string.Join("; ", scene.Differences));
        }
        Fails(Plan("""{"view": {"$component": [0, 1]}}""", unplanned: animator), "A.Rules.view: a Camera field references a Animator");
        Fails(Plan(reference, unplanned: """{"type": "GraphicRaycaster"}"""), "component [0, 1] has no engine object scripts can reference");
    }

    internal static void Fails(string plan, string expected, SceneLoadOptions? options = null)
    {
        var before = UnityEngine.Object.FindObjectsByType<Rules>(FindObjectsSortMode.None).Length;
        try
        {
            DisunityHost.LoadScene(plan, options ?? Options).Dispose();
            throw new InvalidOperationException($"loading succeeded; expected {expected}");
        }
        catch (SceneLoadException error)
        {
            Require(error.Message.Contains(expected), $"expected `{expected}` in `{error.Message}`");
        }
        Require(UnityEngine.Object.FindObjectsByType<Rules>(FindObjectsSortMode.None).Length == before, "a failed load leaves no objects");
    }

    private static void Expect(Func<float> read, string expected)
    {
        try
        {
            read();
            throw new InvalidOperationException($"expected failure `{expected}`");
        }
        catch (ArgumentException error)
        {
            Require(error.Message.Contains(expected), error.Message);
        }
    }
}
