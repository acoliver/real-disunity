using Disunity.Hosting;
using UnityEngine;
using EngineObject = UnityEngine.Object;

internal static class PendingCloneTests
{
    private sealed class Probe : MonoBehaviour
    {
        public Probe? Link;
        public static Action<Probe>? Tick;
        public static int Activations;
        private void Awake() => Activations++;
        private void Update() => Tick?.Invoke(this);
    }

    private sealed class Failing : MonoBehaviour
    {
        public static bool Fail;
        public Failing() { if (Fail) throw new InvalidOperationException("pending clone failure"); }
    }

    public static void Run()
    {
        PendingParent();
        PendingSource();
        PendingChild();
        PendingParentRollback();
        Console.WriteLine("PASS: pending clone ownership, complete source copies and rollback");
    }

    private static void PendingParent()
    {
        DisunityHost.LoadWorld();
        var template = DisunityHost.AddComponent(new Probe());
        var child = DisunityHost.AddComponent(new Probe());
        child.transform.SetParent(template.transform, false);
        template.Link = child;
        var parent = DisunityHost.AddComponent(new Probe()).gameObject;
        EngineObject.Destroy(parent);
        var before = Probe.Activations;
        var clone = EngineObject.Instantiate(template, parent.transform);
        var cloneChild = clone.Link!;
        if (Probe.Activations != before) throw new Exception("pending parent clone activated");
        var ticks = 0;
        Probe.Tick = value => { if (ReferenceEquals(value, clone) || ReferenceEquals(value, cloneChild)) ticks++; };
        DisunityHost.Step(0);
        Probe.Tick = null;
        if (ticks != 0 || clone || clone.gameObject || cloneChild || cloneChild.gameObject)
            throw new Exception("pending parent clone survived or ticked");
        DisunityHost.UnloadWorld();
    }

    private static void PendingSource()
    {
        DisunityHost.LoadWorld();
        var source = DisunityHost.AddComponent(new Probe());
        source.Link = source;
        Probe? componentClone = null;
        GameObject? objectClone = null;
        Probe.Tick = value =>
        {
            if (!ReferenceEquals(value, source)) return;
            EngineObject.Destroy(source.gameObject);
            objectClone = EngineObject.Instantiate(source.gameObject);
            componentClone = EngineObject.Instantiate(source);
        };
        try { DisunityHost.Step(0); }
        finally { Probe.Tick = null; }
        var objectProbe = objectClone!.GetComponent<Probe>()!;
        if (!componentClone || !objectClone || source || !ReferenceEquals(componentClone!.Link, componentClone)
            || !ReferenceEquals(objectProbe.Link, objectProbe))
            throw new Exception("pending source lost components or remapping");
        DisunityHost.UnloadWorld();
    }

    private static void PendingChild()
    {
        DisunityHost.LoadWorld();
        var root = DisunityHost.AddComponent(new Probe());
        var child = DisunityHost.AddComponent(new Probe());
        child.transform.SetParent(root.transform, false);
        root.Link = child;
        child.Link = root;
        EngineObject.Destroy(child.gameObject);
        var clone = EngineObject.Instantiate(root);
        DisunityHost.Step(0);
        if (!clone.Link || !ReferenceEquals(clone.Link!.Link, clone)
            || clone.Link.transform.parent != clone.transform)
            throw new Exception("pending child copy incomplete");
        DisunityHost.UnloadWorld();
    }

    private static void PendingParentRollback()
    {
        DisunityHost.LoadWorld();
        var template = DisunityHost.AddComponent(new Failing());
        var parent = DisunityHost.AddComponent(new Probe()).gameObject;
        EngineObject.Destroy(parent);
        var count = EngineObject.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length;
        Failing.Fail = true;
        try { EngineObject.Instantiate(template, parent.transform); throw new Exception("failure swallowed"); }
        catch (System.Reflection.TargetInvocationException error) when (error.InnerException?.Message == "pending clone failure") { }
        finally { Failing.Fail = false; }
        if (EngineObject.FindObjectsByType<GameObject>(FindObjectsSortMode.None).Length != count)
            throw new Exception("failed pending clone leaked");
        DisunityHost.Step(0);
        DisunityHost.UnloadWorld();
    }
}
