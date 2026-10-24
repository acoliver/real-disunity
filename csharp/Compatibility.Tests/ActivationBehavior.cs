using CompatibilityBridge;
using UnityEngine;
using UObject = UnityEngine.Object;

internal static class ActivationBehavior
{
    public static void Run()
    {
        var actor = new GameObject("initially-disabled");
        using var payload = TestGraph.Payload([new("disabled", actor, "ActivationProbe", new { })]);
        var handle = payload.RootElement.GetProperty("handles").GetProperty("disabled").GetUInt64();
        NativeWorld.Request("component.set", ("component", handle), ("enabled", false)).Dispose();
        using var session = new ScriptSession(payload.RootElement); session.Initialize(); session.Dispatch("Update");
        BinderBehavior.Check(actor.transform.position == new Vector3(1, 0, 0), "active but disabled script receives Awake, defers OnEnable/Start/Update");
        var script = (MonoBehaviour)session.Resolve("disabled"); script.enabled = true;
        session.Dispatch("FixedUpdate");
        BinderBehavior.Check(actor.transform.position == new Vector3(1, 1, 1), "enable activates once and Start precedes FixedUpdate");
        session.Dispatch("LateUpdate");
        BinderBehavior.Check(actor.transform.localScale == new Vector3(2, 2, 1), "private FixedUpdate and LateUpdate messages execute");
        var parent = new GameObject("inactive-parent"); parent.SetActive(false); actor.transform.SetParent(parent.transform);
        var before = actor.transform.position;
        session.Dispatch("Update");
        BinderBehavior.Check(actor.transform.position == before, "parent inactivity suspends update");
        parent.SetActive(true); session.Dispatch("Update");
        BinderBehavior.Check(actor.transform.position.x == 1 && actor.transform.position.z == 2, "parent activation preserves Awake/Start once and permits next Update");
        var runtime = NativeIdentity.Runtime;
        UObject.Destroy(parent);
        BinderBehavior.Check(script == null, "recursive native destruction invalidates descendant component");
        Console.WriteLine("PASS activation: disabled-before-Awake, deferred Start, FixedUpdate/LateUpdate, ancestor activity and recursive invalidation");
    }
}
public class ActivationProbe : MonoBehaviour
{
    private void Awake() { var p = transform.position; p.x++; transform.position = p; }
    private void OnEnable() { var p = transform.position; p.y++; transform.position = p; }
    private void OnDisable() { var p = transform.position; p.y--; transform.position = p; }
    private void Start() { var p = transform.position; p.z++; transform.position = p; }
    private void Update() { var p = transform.position; p.z++; transform.position = p; }
    private void FixedUpdate() { var s = transform.localScale; s.x++; transform.localScale = s; }
    private void LateUpdate() { var s = transform.localScale; s.y++; transform.localScale = s; }
}
