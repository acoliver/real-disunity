using Disunity.Hosting;
using UnityEngine;

internal static class CloneTests
{
    private sealed class Probe : MonoBehaviour
    {
        public Vector3 Spawn = new(1, 2, 3);
        public Color Tint = new(0.2f, 0.4f, 0.6f, 0.5f);
        public Bounds Box = new(new(1, 2, 3), new(4, 5, 6));
        public LayerMask Mask = 1 << 5;
        public Color[] Tints = { new(0.3f, 0.5f, 0.7f, 0.9f) };
        public List<Bounds> Boxes = new() { new(new(2, 3, 4), new(5, 6, 7)) };
        public Probe? Internal;
        public Probe? External;
        public Probe? Child;
        public GameObject? ObjectReference;
        public Transform? TransformReference;
        public List<Probe> References = new();
        public static bool FailConstruction;
        public Probe() { if (FailConstruction) throw new InvalidOperationException("constructor failure"); }
        public static Action<Probe>? Activation;
        private void Awake() => Activation?.Invoke(this);
    }

    private sealed class Unsupported : MonoBehaviour { public object Value = DateTime.UnixEpoch; }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        var template = DisunityHost.AddComponent(new Probe());
        var outside = DisunityHost.AddComponent(new Probe());
        var spare = DisunityHost.AddComponent(new Probe());
        UnityEngine.Object.Destroy(spare);
        if (!spare.gameObject) throw new Exception("component destruction removed its owner");
        template.Internal = template;
        template.External = outside;
        template.ObjectReference = template.gameObject;
        template.TransformReference = template.transform;
        template.References.Add(template);
        var order = new List<string>();
        Probe.Activation = clone =>
        {
            if (ReferenceEquals(clone, template) || ReferenceEquals(clone, outside)) return;
            if (!clone.Spawn.Equals(template.Spawn) || !ReferenceEquals(clone.Internal, clone) || !ReferenceEquals(clone.External, outside)
                || !ReferenceEquals(clone.ObjectReference, clone.gameObject)
                || !ReferenceEquals(clone.TransformReference, clone.transform)
                || !ReferenceEquals(clone.References.Single(), clone))
                throw new Exception("clone references must be bound before Awake");
            order.Add("awake");
            if (order.Count == 1)
            {
                UnityEngine.Object.Instantiate(template);
                order.Add("nested-return");
            }
        };
        var result = UnityEngine.Object.Instantiate(template);
        order.Add("return");
        if (result.Tint != template.Tint || result.Box != template.Box
            || result.Tints[0] != template.Tints[0] || result.Boxes[0] != template.Boxes[0]
            || ReferenceEquals(result.Tints, template.Tints) || ReferenceEquals(result.Boxes, template.Boxes))
            throw new Exception("Color and Bounds serialized fields and collections must retain values");
        if (result.Mask.value != 1 << 5) throw new Exception("a LayerMask field keeps its value");
        if (ReferenceEquals(template, result) || !order.SequenceEqual(new[] { "awake", "awake", "nested-return", "return" }))
            throw new Exception("nested Instantiate must drain native lifecycle before returning");
        Probe.Activation = null;
        var child = DisunityHost.AddComponent(new Probe());
        child.transform.SetParent(template.transform, false);
        template.Child = child;
        child.Internal = template;
        child.gameObject.SetActive(false);
        var subtree = UnityEngine.Object.Instantiate(template);
        if (ReferenceEquals(subtree.Child, child) || !ReferenceEquals(subtree.Child!.Internal, subtree)
            || !ReferenceEquals(subtree.Child.transform.parent, subtree.transform))
            throw new Exception("native subtree and managed child references must remap together");
        Probe.FailConstruction = true;
        try { UnityEngine.Object.Instantiate(template); throw new Exception("constructor failure lost"); }
        catch (System.Reflection.TargetInvocationException error) when (error.InnerException?.Message == "constructor failure") { }
        finally { Probe.FailConstruction = false; }
        var unsupported = DisunityHost.AddComponent(new Unsupported());
        foreach (var value in new object[] { DateTime.UnixEpoch, new Dictionary<string, int> { ["a"] = 7 }, new int[2, 2] })
        {
            unsupported.Value = value;
            try { UnityEngine.Object.Instantiate(unsupported); throw new Exception("unsupported serialized type silently cloned"); }
            catch (NotSupportedException) { }
        }
        DisunityHost.Step(0);
        DisunityHost.UnloadWorld();
        Console.WriteLine("PASS: native Instantiate, serialized reference remap and nested activation");
    }
}
