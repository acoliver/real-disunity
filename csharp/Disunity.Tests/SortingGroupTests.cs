using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>SortingGroup: defaults, the native settings and enabled state, one per object, RequireComponent, parent lookup and cloning.</summary>
internal static class SortingGroupTests
{
    [RequireComponent(typeof(SortingGroup))]
    private sealed class Widget : MonoBehaviour { }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            Defaults();
            Console.WriteLine("PASS: a SortingGroup is enabled on the Default layer at order 0 and does not sort at the root");
            Settings();
            Console.WriteLine("PASS: SortingGroup layer, order and sortAtRoot read back as written; the order is stored unclamped");
            LayerNames();
            Console.WriteLine("PASS: SortingGroup layer names accept Default and reject unregistered layers");
            EnabledState();
            Console.WriteLine("PASS: SortingGroup.enabled is native state and follows the object's active state");
            OnePerObject();
            Console.WriteLine("PASS: a second SortingGroup on an object is rejected and the first is kept");
            Required();
            Console.WriteLine("PASS: RequireComponent(typeof(SortingGroup)) adds one group before the script, found from a child");
            Cloning();
            Console.WriteLine("PASS: Instantiate gives the clone its own SortingGroup settings");
            Planned();
            Console.WriteLine("PASS: a planned SortingGroup builds with its enabled state, layer, order and sortAtRoot");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static SortingGroup NewGroup() => new GameObject("group").AddComponent<SortingGroup>();

    private static void Defaults()
    {
        var group = NewGroup();
        Expect.That(group.enabled && group.isActiveAndEnabled, "enabled by default");
        Expect.Equal(0, group.sortingLayerID, "layer id");
        Expect.Equal("Default", group.sortingLayerName, "layer name");
        Expect.Equal(0, group.sortingOrder, "order");
        Expect.That(!group.sortAtRoot, "sortAtRoot");
        SortingGroup.UpdateAllSortingGroups();
    }

    private static void Settings()
    {
        var group = NewGroup();
        group.sortingOrder = 70000;
        group.sortAtRoot = true;
        group.sortingLayerID = 5;
        Expect.Equal(70000, group.sortingOrder, "order is stored as given");
        Expect.That(group.sortAtRoot, "sortAtRoot written");
        Expect.Equal(5, group.sortingLayerID, "layer id written");
        group.sortingOrder = -3;
        group.sortAtRoot = false;
        Expect.Equal(-3, group.sortingOrder, "order rewritten");
        Expect.That(!group.sortAtRoot, "sortAtRoot cleared");
        Expect.Equal(5, group.sortingLayerID, "an unrelated write keeps the layer");
    }

    private static void LayerNames()
    {
        var group = NewGroup();
        group.sortingLayerID = 4;
        group.sortingLayerName = "Default";
        Expect.Equal(0, group.sortingLayerID, "setting Default sets id 0");
        Expect.Throws<NotSupportedException>(() => group.sortingLayerName = "Units", "an unregistered name");
        group.sortingLayerID = 2;
        Expect.Throws<NotSupportedException>(() => _ = group.sortingLayerName, "the name of an unregistered id");
    }

    private static void EnabledState()
    {
        var owner = new GameObject("toggle");
        var group = owner.AddComponent<SortingGroup>();
        group.enabled = false;
        Expect.That(!group.enabled && !group.isActiveAndEnabled, "disabled");
        group.enabled = true;
        owner.SetActive(false);
        Expect.That(group.enabled && !group.isActiveAndEnabled, "enabled but the object is inactive");
    }

    private static void OnePerObject()
    {
        var owner = new GameObject("one");
        var first = owner.AddComponent<SortingGroup>();
        first.sortingOrder = 9;
        try { owner.AddComponent<SortingGroup>(); } catch (Exception) { }
        Expect.Equal(1, owner.GetComponents<SortingGroup>().Length, "one group");
        Expect.Equal(9, owner.GetComponent<SortingGroup>()!.sortingOrder, "the first group is kept");
    }

    private static void Required()
    {
        var owner = new GameObject("widget");
        owner.AddComponent<Widget>();
        Expect.Equal(1, owner.GetComponents<SortingGroup>().Length, "the group was required");
        var child = new GameObject("child");
        child.transform.SetParent(owner.transform, false);
        Expect.That(ReferenceEquals(child.GetComponentInParent<SortingGroup>(), owner.GetComponent<SortingGroup>()), "found from the child");
        Expect.That(new GameObject("alone").GetComponentInParent<SortingGroup>() is null, "no group above a lone object");
        Expect.That(!new GameObject("alone2").GetComponentInParent<SortingGroup>(), "a missing group is falsy");
    }

    private static void Cloning()
    {
        var group = NewGroup();
        group.sortingOrder = 12;
        group.sortAtRoot = true;
        group.enabled = false;
        var clone = UnityEngine.Object.Instantiate(group.gameObject).GetComponent<SortingGroup>()!;
        Expect.That(!ReferenceEquals(group, clone), "a new component");
        Expect.That(clone.sortingOrder == 12 && clone.sortAtRoot && !clone.enabled, "settings copied");
        clone.sortingOrder = 1;
        Expect.Equal(12, group.sortingOrder, "the original is unchanged");
    }

    private const string Plan = """
        {"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[
        {"name":"Widget","tag":"Untagged","layer":0,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],"rect":null,"components":[
        {"type":"SortingGroup","enabled":false,"sorting_layer_id":0,"sorting_order":-12,"sort_at_root":true}]}]}
        """;

    private static void Planned()
    {
        var assembly = typeof(SortingGroupTests).Assembly;
        using var loaded = DisunityHost.LoadScene(Plan, new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
        var group = loaded.Objects[0].GetComponent<SortingGroup>()!;
        Expect.That(!group.enabled, "the plan's enabled state");
        Expect.That(group.sortingLayerID == 0 && group.sortingOrder == -12 && group.sortAtRoot, "the plan's layer, order and sortAtRoot");
        Expect.Equal(1, loaded.Objects[0].GetComponents<SortingGroup>().Length, "one group");
    }
}
