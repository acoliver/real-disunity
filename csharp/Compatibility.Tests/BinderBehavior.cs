using System.Collections;
using System.Reflection;
using System.Text.Json;
using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

static class BinderBehavior
{
    public static void Run()
    {
        var actor = new GameObject("binder-actor");
        var target = new GameObject("binder-target");
        target.transform.position = new Vector3(7, 0);
        var script = AddScript(actor, "BindingProbe");
        using var payload = JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            assemblies = new[] { typeof(BinderBehavior).Assembly.Location },
            graph = new
            {
                nodes = new[] { new { id = "actor", template = false, active = true }, new { id = "target", template = false, active = true } },
                components = new object[]
                {
                    new { id = "probe", @object = "actor", script = new { type_name = "BindingProbe", fields = new { target = new Dictionary<string, string> { ["$ref"] = "target" }, amount = 11 } } }
                }, assets = Array.Empty<object>()
            },
            handles = new Dictionary<string, ulong> { ["actor"] = actor.NativeHandle, ["target"] = target.NativeHandle, ["probe"] = script },
            profile = new { id = "compat-6000.3" }
        }));
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        Check(actor.transform.position.x == 18, "private serialized reference/value must bind before inherited private Awake");
        Check(actor.transform.position.y == 1, "OnEnable runs after Awake");
        session.Dispatch("Awake"); session.Dispatch("OnEnable");
        Check(actor.transform.position.y == 1, "initialization phases are idempotent");
        session.Dispatch("Update");
        Check(actor.transform.position.z == 2, "Start runs once before Update and sees cached owner/component identity");
        session.Dispatch("Update");
        Check(actor.transform.position.z == 3, "Update dispatches actual script instance");
        actor.SetActive(false);
        Check(actor.transform.position.y == 0, "native activation dispatches OnDisable");
        session.Dispatch("Update");
        Check(actor.transform.position.z == 3, "inactive scripts do not update");
        actor.SetActive(true);
        Check(actor.transform.position.y == 1, "native reactivation dispatches OnEnable");
        var bound = (MonoBehaviour)session.Resolve("probe");
        Check(ReferenceEquals(actor.GetComponent<MonoBehaviour>(), bound), "GetComponent returns bound instance");
        bound.enabled = false;
        Check(actor.transform.position.y == 0, "enabled uses native component flag");
        bound.enabled = true;
        var clone = UObject.Instantiate(actor);
        Check(clone.GetComponent<MonoBehaviour>() is not null, "clone binds actual managed script");
        Check(clone.transform.position.x == 18 && clone.transform.position.y == 1, "clone references remain external and lifecycle runs after field copy");
        var cloneScript = (MonoBehaviour)clone.GetComponent<MonoBehaviour>()!;
        Check(!ReferenceEquals(cloneScript, bound), "clone script has distinct native identity");
        UObject.Destroy(clone);
        Check(clone == null && cloneScript == null, "destroy invalidates native generation and Unity null");
        Check(actor.transform.position.y == 1, "destroying clone preserves original");
        Console.WriteLine("PASS binder: fields-before-Awake, inherited private messages, cache, phase idempotence, enable, clone and stale handles");
    }
    internal static ulong AddScript(GameObject owner, string type)
    {
        using var result = NativeWorld.Request("component.add", ("object", owner.NativeHandle), ("kind", "Script"), ("fields", new { type }));
        return NativeWorld.Data(result).GetProperty("handle").GetUInt64();
    }
    internal static void Check(bool condition, string message) { if (!condition) throw new Exception(message); }
}

public class BindingProbeBase : MonoBehaviour
{
    [SerializeField] private GameObject target = null!;
    [SerializeField] private int amount = 0;
    private void Awake()
    {
        if (!ReferenceEquals(this, GetComponent<BindingProbeBase>()) || !ReferenceEquals(transform, gameObject.transform)) throw new Exception("broken component identity");
        transform.position = new Vector3(target.transform.position.x + amount, 0, 0);
    }
}
public class BindingProbe : BindingProbeBase
{
    private void OnEnable() { var p = transform.position; p.y++; transform.position = p; }
    private void OnDisable() { var p = transform.position; p.y--; transform.position = p; }
    private void Start() { var p = transform.position; p.z++; transform.position = p; }
    private void Update() { var p = transform.position; p.z++; transform.position = p; }
}
