using System.Diagnostics;
using Disunity.Hosting;
using UnityEngine;
using static GraphicsTestSupport;
using EngineObject = UnityEngine.Object;

/// <summary>Child lists, sibling order, descendant searches and handle lookup, on small trees and on a world of
/// thousands of objects where each query must cost what its subtree costs.</summary>
internal static class HierarchyQueryTests
{
    private sealed class Tag : MonoBehaviour { }

    private const string ScenePlan = """
        {"scene":"tree","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"A","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[]},
        {"name":"A1","tag":"Untagged","layer":0,"active":true,"parent":0,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[]},
        {"name":"A2","tag":"Untagged","layer":0,"active":true,"parent":0,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[]}]}
        """;

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            OrderFollowsReparenting();
            DescendantsAreDepthFirstInSiblingOrder();
            InactiveObjectsAreFilteredAsBefore();
            DestroyedObjectsLeaveTheirParent();
            ResolveObjectFindsLiveObjectsOnly();
        }
        finally { DisunityHost.UnloadWorld(); }
        SceneUnloadForgetsTheScene();
        LargeWorld();
        Console.WriteLine("PASS: hierarchy queries keep sibling order through reparenting, destroy and unload, and answer on a world of thousands");
    }

    private static Transform Make(string name, Transform? parent = null)
    {
        var value = DisunityHost.CreateGameObject();
        value.name = name;
        value.AddComponent<Tag>();
        if (parent is not null) value.transform.SetParent(parent, false);
        return value.transform;
    }

    private static string Names(Transform parent) =>
        string.Concat(Enumerable.Range(0, parent.childCount).Select(i => parent.GetChild(i).name));

    private static string Found(Transform root, bool includeInactive = false) =>
        string.Concat(root.GetComponentsInChildren<Tag>(includeInactive).Select(tag => tag.gameObject.name));

    private static void OrderFollowsReparenting()
    {
        var (one, two) = (Make("one"), Make("two"));
        var (a, b, c) = (Make("a", one), Make("b", one), Make("c", one));
        Ensure(Names(one) == "abc" && Names(two) == "", "children come in creation order");
        b.SetParent(two, false);
        Ensure(Names(one) == "ac" && Names(two) == "b", "a moved child leaves its old parent");
        a.SetParent(two, false);
        Ensure(Names(two) == "ba" && a.GetSiblingIndex() == 1, "a moved child comes last under its new parent");
        a.SetAsFirstSibling();
        Ensure(Names(two) == "ab", "SetAsFirstSibling");
        a.SetAsLastSibling();
        c.SetParent(two, false);
        c.SetSiblingIndex(1);
        Ensure(Names(two) == "bca" && c.GetSiblingIndex() == 1 && one.childCount == 0, "SetSiblingIndex among the new siblings");
        a.SetParent(null, true);
        Ensure(Names(two) == "bc" && a.parent is null, "a root has no parent");
    }

    private static void DescendantsAreDepthFirstInSiblingOrder()
    {
        var top = Make("t");
        var (x, y) = (Make("x", top), Make("y", top));
        Make("x1", x);
        var x2 = Make("x2", x);
        Make("x2a", x2);
        Make("y1", y);
        Ensure(Found(top) == "txx1x2x2ayy1", "depth first, parents before children");
        y.SetAsFirstSibling();
        Ensure(Found(top) == "tyy1xx1x2x2a", "a sibling moved first is searched first");
        Ensure(Found(x2) == "x2x2a" && Found(top.Find("y/y1")!) == "y1", "a search starts at its own object");
    }

    private static void InactiveObjectsAreFilteredAsBefore()
    {
        var top = Make("t");
        var middle = Make("m", top);
        Make("leaf", middle);
        Make("side", top);
        middle.gameObject.SetActive(false);
        Ensure(Found(top) == "tside", "an inactive object hides its subtree");
        Ensure(Found(top, true) == "tmleafside", "includeInactive brings the subtree back in order");
        Ensure(Names(top) == "mside" && top.childCount == 2, "inactive children still count");
    }

    private static void DestroyedObjectsLeaveTheirParent()
    {
        var top = Make("t");
        var (a, b, c) = (Make("a", top), Make("b", top), Make("c", top));
        var below = Make("a1", a);
        EngineObject.Destroy(b.gameObject);
        DisunityHost.Step(0);
        Ensure(Names(top) == "ac" && c.GetSiblingIndex() == 1, "a destroyed child is gone and the rest close up");
        EngineObject.Destroy(a.gameObject);
        DisunityHost.Step(0);
        Ensure(Names(top) == "c" && top.childCount == 1 && !below, "destroying a parent takes its children");
        Ensure(Found(top) == "tc", "searches no longer see destroyed objects");
    }

    private static void ResolveObjectFindsLiveObjectsOnly()
    {
        var kept = Make("kept");
        var gone = Make("gone", kept);
        Ensure(ReferenceEquals(DisunityHost.ResolveObject(gone.gameObject.RawHandle), gone.gameObject), "a live handle resolves to its object");
        var handle = gone.gameObject.RawHandle;
        EngineObject.Destroy(gone.gameObject);
        DisunityHost.Step(0);
        Throws<InvalidOperationException>(() => DisunityHost.ResolveObject(handle), "a destroyed object does not resolve", "no live object");
        Throws<InvalidOperationException>(() => DisunityHost.ResolveObject(0xdead_beefUL), "an unknown handle does not resolve", "no live object");
        Ensure(ReferenceEquals(DisunityHost.ResolveObject(kept.gameObject.RawHandle), kept.gameObject), "other objects still resolve");
    }

    private static void SceneUnloadForgetsTheScene()
    {
        DisunityHost.LoadWorld();
        ulong stale;
        try
        {
            using var loaded = DisunityHost.LoadScene(ScenePlan, new SceneLoadOptions(typeof(HierarchyQueryTests).Assembly, _ => []));
            var root = GameObject.Find("A")!.transform;
            Ensure(Names(root) == "A1A2" && root.GetChild(1).GetSiblingIndex() == 1, "a loaded scene's children keep plan order");
            stale = root.gameObject.RawHandle;
        }
        finally { DisunityHost.UnloadWorld(); }
        DisunityHost.LoadWorld();
        try
        {
            Ensure(GameObject.Find("A") is null, "the next world has no scene objects");
            Throws<InvalidOperationException>(() => DisunityHost.ResolveObject(stale), "an object of an unloaded world does not resolve", "no live object");
            Ensure(Names(Make("fresh")) == "", "a new world answers child queries");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private const int Branches = 10, Twigs = 10, Leaves = 29, Rounds = 20;

    /// <summary>A root with 10 branches of 10 twigs of 29 leaves (3110 objects) and a chain 200 deep.</summary>
    private static Transform BuildLarge(out Transform deepest)
    {
        var root = Make("root");
        for (var b = 0; b < Branches; b++)
        {
            var branch = Make($"b{b}", root);
            for (var t = 0; t < Twigs; t++)
            {
                var twig = Make($"b{b}t{t}", branch);
                for (var l = 0; l < Leaves; l++) Make($"b{b}t{t}l{l}", twig);
            }
        }
        deepest = Make("d0", root);
        for (var d = 1; d < 200; d++) deepest = Make($"d{d}", deepest);
        return root;
    }

    private static int Query(Transform root)
    {
        var twig = root.GetChild(3).GetChild(4);
        var leaf = twig.GetChild(7);
        var work = twig.GetComponentsInChildren<Tag>(true).Length + twig.childCount + leaf.GetSiblingIndex();
        return work + root.GetChild(9).childCount;
    }

    private static void LargeWorld()
    {
        DisunityHost.LoadWorld();
        try
        {
            var root = BuildLarge(out var deepest);
            var twig = root.GetChild(3).GetChild(4);
            var found = twig.GetComponentsInChildren<Tag>(true);
            Ensure(found.Length == 1 + Leaves && found[0].gameObject.name == "b3t4" && found[Leaves].gameObject.name == "b3t4l28", "a twig's subtree in order");
            Ensure(twig.childCount == Leaves && twig.GetChild(7).name == "b3t4l7" && twig.GetChild(7).GetSiblingIndex() == 7, "childCount, GetChild and GetSiblingIndex in a wide tree");
            Ensure(root.childCount == Branches + 1 && root.GetChild(Branches).name == "d0", "the root's children");
            Ensure(deepest.GetComponentsInChildren<Tag>(true).Length == 1 && deepest.root == root, "a deep chain ends at its leaf");
            Ensure(root.GetChild(Branches).GetComponentsInChildren<Tag>(true).Length == 200, "a deep chain is searched to its end");
            var watch = Stopwatch.StartNew();
            var total = 0;
            for (var i = 0; i < Rounds; i++) total += Query(root);
            watch.Stop();
            Ensure(total == Rounds * (1 + Leaves + Leaves + 7 + Twigs), "the timed queries return the same answers every time");
            Console.WriteLine($"TIMING: {Rounds} subtree queries in a world of {Branches * Twigs * (Leaves + 1) + Branches + 201} objects took {watch.ElapsedMilliseconds} ms");
        }
        finally { DisunityHost.UnloadWorld(); }
    }
}
