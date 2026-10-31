using Disunity.Hosting;
using UnityEngine;

/// <summary>AddComponent adds the components a script's RequireComponent attributes name, before the script itself.</summary>
internal static class RequireComponentTests
{
    private static readonly List<string> Seen = [];

    [RequireComponent(typeof(RectTransform))]
    private class NeedsRect : MonoBehaviour
    {
        private void Awake() => Seen.Add($"NeedsRect sees rect={GetComponent<RectTransform>() is not null}");
    }

    private sealed class DerivedNeedsRect : NeedsRect { }

    [RequireComponent(typeof(ChainB))]
    private sealed class ChainA : MonoBehaviour
    {
        private void Awake() => Seen.Add($"A sees B={GetComponent<ChainB>() is not null} C={GetComponent<ChainC>() is not null}");
    }

    [RequireComponent(typeof(ChainC))]
    private sealed class ChainB : MonoBehaviour
    {
        private void Awake() => Seen.Add("B");
    }

    private sealed class ChainC : MonoBehaviour
    {
        private void Awake() => Seen.Add("C");
    }

    [RequireComponent(typeof(Camera))]
    private sealed class NeedsCamera : MonoBehaviour { }

    [RequireComponent(typeof(CanvasRenderer), typeof(Camera))]
    private sealed class NeedsTwo : MonoBehaviour { }

    [RequireComponent(typeof(CanvasRenderer), typeof(Camera), typeof(CanvasGroup))]
    private sealed class NeedsThree : MonoBehaviour { }

    [RequireComponent(typeof(Collider2D))]
    private sealed class NeedsAbstract : MonoBehaviour
    {
        private void Awake() => Seen.Add("NeedsAbstract awake");
    }

    [RequireComponent(typeof(CycleB))]
    private sealed class CycleA : MonoBehaviour { }

    [RequireComponent(typeof(CycleA))]
    private sealed class CycleB : MonoBehaviour { }

    [RequireComponent(typeof(Transform))]
    private sealed class NeedsTransform : MonoBehaviour { }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            RectTransformReplacesTransformKeepingPose();
            Console.WriteLine("PASS: RequireComponent(RectTransform) turns a plain transform into a RectTransform and keeps parent, children and pose");
            InheritedRequirements();
            Console.WriteLine("PASS: RequireComponent is inherited from base classes");
            Chain();
            Console.WriteLine("PASS: a chain of requirements adds C, B, A in that order and A's Awake sees B and C");
            ExistingComponentsAreKept();
            Console.WriteLine("PASS: a required component the object already has is not added again");
            ManyTypes();
            Console.WriteLine("PASS: the two- and three-type RequireComponent constructors add each type once");
            AbstractRequirement();
            Console.WriteLine("PASS: an abstract required type with no match logs an error naming both types and the script is still added");
            Cycle();
            Console.WriteLine("PASS: requirements that name each other add both once");
            Expect.Equal(0, new GameObject("plain").GetComponents<Component>().Length - 1, "a plain object has only its transform");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void RectTransformReplacesTransformKeepingPose()
    {
        var parent = new GameObject("parent");
        var owner = new GameObject("owner");
        var sibling = new GameObject("sibling");
        var child = new GameObject("child");
        owner.transform.SetParent(parent.transform, false);
        sibling.transform.SetParent(parent.transform, false);
        child.transform.SetParent(owner.transform, false);
        owner.transform.SetSiblingIndex(1);
        owner.transform.localPosition = new Vector3(3, 4, 5);
        owner.transform.localRotation = Quaternion.Euler(0, 0, 30);
        owner.transform.localScale = new Vector3(2, 3, 4);
        var before = owner.transform;
        Expect.That(before is not RectTransform, "the object starts with a plain transform");
        owner.AddComponent<NeedsRect>();
        var rect = owner.transform as RectTransform;
        Expect.That(rect is not null, "the transform is now a RectTransform");
        Expect.That(ReferenceEquals(owner.GetComponent<Transform>(), rect), "GetComponent<Transform> returns the RectTransform");
        Expect.That(ReferenceEquals(owner.GetComponent<RectTransform>(), rect), "GetComponent<RectTransform> returns it too");
        Expect.That(owner.transform.parent == parent.transform, "the parent is kept");
        Expect.Equal(1, owner.transform.GetSiblingIndex(), "sibling index");
        Expect.Equal(1, owner.transform.childCount, "child count");
        Expect.That(owner.transform.GetChild(0) == child.transform, "the child is kept");
        Expect.That(child.transform.parent == owner.transform, "the child's parent is the object");
        Expect.That(owner.transform.localPosition == new Vector3(3, 4, 5), $"local position {owner.transform.localPosition}");
        Expect.Near(30f, owner.transform.localRotation.eulerAngles.z, "local rotation");
        Expect.That(owner.transform.localScale == new Vector3(2, 3, 4), $"local scale {owner.transform.localScale}");
        Expect.Equal("NeedsRect sees rect=True", Seen[^1], "Awake sees the RectTransform");
        Expect.Equal(1, owner.GetComponents<Transform>().Length, "one transform");
    }

    private static void InheritedRequirements()
    {
        var owner = new GameObject("derived");
        owner.AddComponent<DerivedNeedsRect>();
        Expect.That(owner.transform is RectTransform, "the base class requirement was applied");
    }

    private static void Chain()
    {
        Seen.Clear();
        var owner = new GameObject("chain");
        owner.AddComponent<ChainA>();
        Expect.Sequence(["C", "B", "A sees B=True C=True"], Seen, "awake order");
        Expect.Equal(1, owner.GetComponents<ChainC>().Length, "one C");
    }

    private static void ExistingComponentsAreKept()
    {
        var owner = new GameObject("existing");
        var camera = owner.AddComponent<Camera>();
        owner.AddComponent<NeedsCamera>();
        Expect.Equal(1, owner.GetComponents<Camera>().Length, "one camera");
        Expect.That(ReferenceEquals(owner.GetComponent<Camera>(), camera), "the camera is the original");
        var ui = new GameObject("ui");
        var rect = ui.AddComponent<RectTransform>();
        ui.AddComponent<NeedsRect>();
        Expect.That(ReferenceEquals(ui.transform, rect), "the RectTransform is the original");
        owner.AddComponent<NeedsTransform>();
        Expect.Equal(1, owner.GetComponents<Transform>().Length, "a Transform requirement adds nothing");
    }

    private static void ManyTypes()
    {
        var two = new GameObject("two");
        two.AddComponent<NeedsTwo>();
        Expect.Equal(1, two.GetComponents<CanvasRenderer>().Length, "renderer of two");
        Expect.Equal(1, two.GetComponents<Camera>().Length, "camera of two");
        var three = new GameObject("three", typeof(NeedsThree));
        Expect.Equal(1, three.GetComponents<CanvasRenderer>().Length, "renderer of three");
        Expect.Equal(1, three.GetComponents<Camera>().Length, "camera of three");
        Expect.Equal(1, three.GetComponents<CanvasGroup>().Length, "group of three");
    }

    private static void AbstractRequirement()
    {
        var errors = new List<string>();
        void Collect(string condition, string stack, LogType type) { if (type == LogType.Error) errors.Add(condition); }
        Application.logMessageReceived += Collect;
        Seen.Clear();
        GameObject owner;
        try { owner = new GameObject("abstract"); owner.AddComponent<NeedsAbstract>(); }
        finally { Application.logMessageReceived -= Collect; }
        Expect.Equal(1, errors.Count, "one error was logged");
        Expect.That(errors[0].Contains("NeedsAbstract") && errors[0].Contains("Collider2D"), $"the error names both types: {errors[0]}");
        Expect.Equal(1, owner.GetComponents<NeedsAbstract>().Length, "the script was still added");
        Expect.Equal("NeedsAbstract awake", Seen[^1], "the script woke");
        owner.AddComponent<BoxCollider2D>();
        var satisfied = new GameObject("satisfied");
        satisfied.AddComponent<BoxCollider2D>();
        Application.logMessageReceived += Collect;
        errors.Clear();
        try { satisfied.AddComponent<NeedsAbstract>(); }
        finally { Application.logMessageReceived -= Collect; }
        Expect.Equal(0, errors.Count, "a collider that matches the abstract type satisfies it");
    }

    private static void Cycle()
    {
        var owner = new GameObject("cycle");
        owner.AddComponent<CycleA>();
        Expect.Equal(1, owner.GetComponents<CycleA>().Length, "one A");
        Expect.Equal(1, owner.GetComponents<CycleB>().Length, "one B");
    }
}
