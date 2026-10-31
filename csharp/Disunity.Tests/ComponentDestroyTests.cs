using Disunity.Hosting;
using UnityEngine;
using Object = UnityEngine.Object;

/// <summary>Destroy and DestroyImmediate for components and objects: Destroy keeps the target in queries until
/// the frame's end and runs its messages in the frame's cleanup; DestroyImmediate hides it and runs the
/// messages before returning.</summary>
internal static class ComponentDestroyTests
{
    private static readonly List<string> Calls = new();

    private class Probe : MonoBehaviour
    {
        public Action? OnUpdate;
        private void OnDisable() => Calls.Add(name + ":OnDisable");
        private void OnDestroy() => Calls.Add(name + ":OnDestroy");
        private void Update() => OnUpdate?.Invoke();
    }

    private sealed class Related : Probe { }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            ScriptDestroyWaitsForTheFrame();
            NativeComponentDestroy();
            ComponentOfARetiredObject();
            TransformIsRefused();
            ReplaceInOneCall();
            DelayedDestroyOfAComponent();
            ImmediateComponent();
            ImmediateObject();
            ImmediateInsideACallback();
            ScriptAssetsStillRefuseImmediate();
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static GameObject Named(string name, Transform? parent = null)
    {
        var value = new GameObject(name);
        if (parent is not null) value.transform.SetParent(parent);
        return value;
    }

    private static void ScriptDestroyWaitsForTheFrame()
    {
        Calls.Clear();
        var owner = Named("a");
        var probe = owner.AddComponent<Probe>();
        DisunityHost.Step(0.01f);
        Object.Destroy(probe);
        Object.Destroy(probe);
        Expect.That(probe != null && owner.GetComponent<Probe>() == probe, "a pending component is still found");
        Expect.Equal(0, Calls.Count, "no messages before the frame's cleanup");
        DisunityHost.Step(0.01f);
        Expect.Sequence(["a:OnDisable", "a:OnDestroy"], Calls, "cleanup messages run once");
        Expect.That(probe == null && owner.GetComponent<Probe>() == null, "a retired component reads null and is not found");
        Expect.That(owner != null && owner.transform != null, "the object stays");
        Object.Destroy(probe);
        DisunityHost.Step(0.01f);
        Expect.Equal(2, Calls.Count, "destroying a retired component is harmless");
        Console.WriteLine("PASS: Object.Destroy(component) keeps a script until the frame ends, runs its messages once, then reads null");
    }

    private static void NativeComponentDestroy()
    {
        var owner = Named("n");
        var group = owner.AddComponent<CanvasGroup>();
        Object.Destroy(group);
        Expect.That(group != null && owner.GetComponent<CanvasGroup>() == group, "a pending native component is still found");
        DisunityHost.Step(0.01f);
        Expect.That(group == null && owner.GetComponent<CanvasGroup>() == null, "a retired native component reads null");
        Expect.That(owner.AddComponent<CanvasGroup>() != null, "a new one can be added afterwards");
        Console.WriteLine("PASS: Object.Destroy(component) retires a native component at the frame's end");
    }

    private static void ComponentOfARetiredObject()
    {
        var owner = Named("gone");
        var probe = owner.AddComponent<Probe>();
        var group = owner.AddComponent<CanvasGroup>();
        DisunityHost.Step(0.01f);
        Object.Destroy(owner);
        DisunityHost.Step(0.01f);
        Expect.That(probe == null && group == null, "the components of a retired object read null");
        Object.Destroy(probe);
        Object.DestroyImmediate(group);
        Object.DestroyImmediate(probe);
        DisunityHost.Step(0.01f);
        Console.WriteLine("PASS: destroying a component of a retired object is harmless");
    }

    private static void TransformIsRefused()
    {
        var owner = Named("t");
        var errors = new List<string>();
        void Collect(string text, string trace, LogType type) { if (type == LogType.Error) errors.Add(text); }
        Application.logMessageReceived += Collect;
        try
        {
            Object.Destroy(owner.transform);
            Object.DestroyImmediate(owner.transform);
        }
        finally { Application.logMessageReceived -= Collect; }
        DisunityHost.Step(0.01f);
        Expect.Equal(2, errors.Count, "each refusal logs an error");
        Expect.That(errors.All(text => text.Contains("Transform")), "the error names the Transform");
        Expect.That(owner.transform != null && owner != null, "the transform and its object stay");
        Console.WriteLine("PASS: destroying a Transform logs an error and changes nothing");
    }

    private static void ReplaceInOneCall()
    {
        var owner = Named("r");
        var old = owner.AddComponent<Probe>();
        Object.Destroy(old);
        var replacement = owner.AddComponent<Related>();
        DisunityHost.Step(0.01f);
        Expect.That(old == null && replacement != null, "the old component retired and the new one stays");
        Expect.That(owner.GetComponent<Probe>() == replacement && owner.GetComponents<Probe>().Length == 1, "only the replacement is found");
        Console.WriteLine("PASS: a component destroyed and replaced in one call leaves only the replacement");
    }

    private static void DelayedDestroyOfAComponent()
    {
        var owner = Named("d");
        var probe = owner.AddComponent<Probe>();
        var error = Expect.Catch<NotSupportedException>(() => Object.Destroy(probe, 1f), "delayed component destruction");
        Expect.That(error.Message.Contains("component"), "the message names components");
        Object.Destroy(probe, 0f);
        DisunityHost.Step(0.01f);
        Expect.That(probe == null, "a zero delay destroys at the frame's end");
        Console.WriteLine("PASS: a delayed component destruction is refused by name, a zero delay is Destroy");
    }

    private static void ImmediateComponent()
    {
        Calls.Clear();
        var owner = Named("i");
        var probe = owner.AddComponent<Probe>();
        var other = owner.AddComponent<CanvasGroup>();
        DisunityHost.Step(0.01f);
        Object.DestroyImmediate(probe);
        Expect.That(probe == null && owner.GetComponent<Probe>() == null && owner.GetComponents<Component>().All(c => c != probe), "hidden at once");
        Expect.Sequence(["i:OnDisable", "i:OnDestroy"], Calls, "messages run before DestroyImmediate returns");
        Object.DestroyImmediate(probe);
        Object.DestroyImmediate(other);
        Expect.That(other == null && owner.GetComponent<CanvasGroup>() == null, "a native component hides at once");
        DisunityHost.Step(0.01f);
        Expect.Equal(2, Calls.Count, "the frame does not repeat the messages");
        Console.WriteLine("PASS: DestroyImmediate(component) hides it and runs its messages at once");
    }

    private static void ImmediateObject()
    {
        Calls.Clear();
        var parent = Named("p");
        var child = Named("c", parent.transform);
        var grandchild = Named("g", child.transform);
        child.AddComponent<Probe>();
        grandchild.AddComponent<Probe>();
        DisunityHost.Step(0.01f);
        Object.DestroyImmediate(child);
        Expect.That(child == null && grandchild == null, "the object and its descendants read null");
        Expect.Equal(0, parent.transform.childCount, "the parent has no children");
        Expect.That(parent.GetComponentsInChildren<Probe>(true).Length == 0, "no script is found below the parent");
        Expect.That(Object.FindObjectsByType<Probe>(FindObjectsSortMode.None).All(p => p.gameObject != grandchild), "a search does not find the descendant");
        Expect.Equal(4, Calls.Count, "both scripts got OnDisable and OnDestroy");
        DisunityHost.Step(0.01f);
        Expect.Equal(4, Calls.Count, "the frame does not repeat them");
        Console.WriteLine("PASS: DestroyImmediate(GameObject) hides the subtree and runs its messages at once");
    }

    private static void ImmediateInsideACallback()
    {
        Calls.Clear();
        var victim = Named("v");
        victim.AddComponent<Probe>();
        var driver = Named("w").AddComponent<Probe>();
        var seen = new List<string>();
        driver.OnUpdate = () =>
        {
            if (seen.Count > 0) return;
            Object.DestroyImmediate(victim);
            seen.Add(string.Join(",", Calls));
        };
        DisunityHost.Step(0.01f);
        Expect.Sequence(["v:OnDisable,v:OnDestroy"], seen, "the messages ran inside the callback");
        Expect.That(victim == null, "the victim reads null");
        DisunityHost.Step(0.01f);
        Expect.Equal(2, Calls.Count, "no repeat");
        Console.WriteLine("PASS: DestroyImmediate inside a callback runs the messages inside it");
    }

    private static void ScriptAssetsStillRefuseImmediate()
    {
        var asset = ScriptableObject.CreateInstance<ScriptableObject>();
        Expect.Throws<NotSupportedException>(() => Object.DestroyImmediate(asset), "DestroyImmediate of a script asset");
        Object.Destroy(asset);
        Console.WriteLine("PASS: DestroyImmediate still refuses script assets");
    }
}
