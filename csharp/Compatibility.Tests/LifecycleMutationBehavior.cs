using CompatibilityBridge;
using UnityEngine;

internal static class LifecycleMutationBehavior
{
    public static void Phase(string phase, bool fail = false)
    {
        var controller = new GameObject("phase-controller");
        var target = new GameObject("phase-target");
        using var payload = TestGraph.Payload([
            new("controller", controller, "LifecyclePhaseMutationProbe", new { target = TestGraph.Ref("target"), mutatePhase = phase, throwAfterMutation = fail }),
            new("target", target, "LifecyclePhaseMutationProbe", new { })]);
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize();
        if (phase != "Start") session.Dispatch("Start");
        target.transform.position = Vector3.zero;
        session.Dispatch(phase);
        BinderBehavior.Check(target.transform.position == Vector3.zero, phase + " callback must prevent a later disabled peer from receiving that phase");
        var controllerScript = session.Resolve("controller");
        controllerScript.GetType().GetField("mutatePhase")!.SetValue(controllerScript, string.Empty);
        ((MonoBehaviour)session.Resolve("target")).enabled = true;
        session.Dispatch(phase);
        BinderBehavior.Check(target.transform.position.x == 1, phase + " must observe native reactivation and deliver the deferred callback once");
        Console.WriteLine("PASS lifecycle phase mutation: " + phase + " refreshes native eligibility after callback");
    }

    public static void Run()
    {
        var awakeController = new GameObject("awake-controller");
        var awakeTarget = new GameObject("awake-target");
        var enableController = new GameObject("enable-controller");
        var enableTarget = new GameObject("enable-target");
        using var payload = TestGraph.Payload([
            new("awake-controller-script", awakeController, "LifecycleMutationProbe", new { target = TestGraph.Ref("awake-target"), disableOnAwake = true }),
            new("awake-target-script", awakeTarget, "LifecycleMutationProbe", new { }),
            new("enable-controller-script", enableController, "LifecycleMutationProbe", new { target = TestGraph.Ref("enable-target"), disableOnEnable = true }),
            new("enable-target-script", enableTarget, "LifecycleMutationProbe", new { })
        ], new() { ["awake-target"] = awakeTarget, ["enable-target"] = enableTarget });
        using var session = new ScriptSession(payload.RootElement);
        session.Initialize(); session.Dispatch("Start");
        BinderBehavior.Check(awakeTarget.transform.position == Vector3.zero, "Awake deactivation must prevent a later peer's Awake, enable and Start in the same reconciliation");
        BinderBehavior.Check(enableTarget.transform.position == new Vector3(1, 0, 0), "OnEnable deactivation must prevent a later awakened peer's OnEnable and Start");
        awakeTarget.SetActive(true); enableTarget.SetActive(true); session.Dispatch("Start");
        BinderBehavior.Check(awakeTarget.transform.position == Vector3.one && enableTarget.transform.position == Vector3.one,
            "fresh reconciliation must observe native peer reactivation and deliver each deferred lifecycle message once");
        session.Dispatch("Start");
        BinderBehavior.Check(awakeTarget.transform.position == Vector3.one && enableTarget.transform.position == Vector3.one, "repeated phases must preserve once-only lifecycle after callback mutations");
        Console.WriteLine("PASS lifecycle callback mutations: native Awake/OnEnable peer deactivation, fresh reactivation and Start once");
    }
}
public class LifecycleMutationProbe : MonoBehaviour
{
    public GameObject? target;
    public bool disableOnAwake;
    public bool disableOnEnable;
    private void Awake()
    {
        var position = transform.position;
        position.x++;
        transform.position = position;
        if (disableOnAwake) target!.SetActive(false);
    }
    private void OnEnable()
    {
        var position = transform.position;
        position.y++;
        transform.position = position;
        if (disableOnEnable) target!.SetActive(false);
    }
    private void Start() { var position = transform.position; position.z++; transform.position = position; }
}

public class LifecyclePhaseMutationProbe : MonoBehaviour
{
    public MonoBehaviour? target;
    public string mutatePhase = string.Empty;
    public bool throwAfterMutation = false;
    private void Start() => Touch("Start");
    private void FixedUpdate() => Touch("FixedUpdate");
    private void Update() => Touch("Update");
    private void LateUpdate() => Touch("LateUpdate");
    private void Touch(string phase)
    {
        transform.position += new Vector3(1, 0);
        if (mutatePhase == phase)
        {
            NativeWorld.Request("component.set", ("component", target!.NativeHandle), ("enabled", false)).Dispose();
            if (throwAfterMutation) throw new InvalidOperationException("mutation callback sentinel");
        }
    }
}
