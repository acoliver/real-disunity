using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>The sorting layer table: the API, a player plan's layers, and the names renderers, groups and canvases take.</summary>
internal static class SortingLayerTests
{
    private const string PlayerLayers = """[{"name":"Back","id":7},{"name":"Default","id":0},{"name":"Units","id":-5},{"name":"Top","id":9}]""";

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            DefaultTable();
            Console.WriteLine("PASS: the engine starts with the Default sorting layer alone and unknown names and ids give 0 and empty");
            PlayerTable();
            Console.WriteLine("PASS: a player plan registers its layers in drawing order, values count from Default and negative before it");
            PlanWithoutDefault();
            Console.WriteLine("PASS: a plan that lacks Default keeps Default first; a plan without layers keeps the table; duplicates are rejected");
            RoundTrips();
            Console.WriteLine("PASS: renderer, sorting group and canvas layer names round-trip through registered layers");
            UnregisteredNames();
            Console.WriteLine("PASS: an unregistered name keeps the layer and warns once per name; an unregistered id is stored");
            CanvasLoads();
            Console.WriteLine("PASS: a canvas on a registered non-zero layer loads from a plan");
        }
        finally
        {
            DisunityHost.ClearSortingLayers();
            DisunityHost.UnloadWorld();
        }
    }

    private static void Register(params (string Name, int Id)[] layers) => DisunityHost.SetSortingLayers(layers);

    private static void DefaultTable()
    {
        Expect.Sequence([0], SortingLayer.layers.Select(l => l.id), "only Default");
        Expect.Equal("Default", SortingLayer.IDToName(0), "Default name");
        Expect.Equal(0, SortingLayer.NameToID("Default"), "Default id");
        Expect.Equal(0, SortingLayer.NameToID("Nope"), "an unknown name gives id 0");
        Expect.Equal("", SortingLayer.IDToName(42), "an unknown id gives the empty name");
        Expect.Equal(0, SortingLayer.GetLayerValueFromName("Nope"), "an unknown name has value 0");
        Expect.Equal(0, SortingLayer.GetLayerValueFromID(42), "an unknown id has value 0");
        Expect.That(SortingLayer.IsValid(0) && !SortingLayer.IsValid(42), "IsValid");
        Expect.Throws<ArgumentNullException>(() => SortingLayer.NameToID(null!), "a null name");
    }

    private static void PlayerTable()
    {
        Register(("Back", 7), ("Default", 0), ("Units", -5), ("Top", 9));
        var layers = SortingLayer.layers;
        Expect.Sequence(["Back", "Default", "Units", "Top"], layers.Select(l => l.name), "drawing order");
        Expect.Sequence([7, 0, -5, 9], layers.Select(l => l.id), "ids");
        Expect.Sequence([-1, 0, 1, 2], layers.Select(l => l.value), "values relative to Default");
        Expect.Equal(-5, SortingLayer.NameToID("Units"), "name to a negative id");
        Expect.Equal("Top", SortingLayer.IDToName(9), "id to name");
        Expect.Equal(-1, SortingLayer.GetLayerValueFromID(7), "a layer before Default");
        Expect.Equal(2, SortingLayer.GetLayerValueFromName("Top"), "a layer after Default");
        Expect.That(SortingLayer.IsValid(-5) && !SortingLayer.IsValid(8), "IsValid for the plan's ids");
        var plan = $$"""{"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[],"sorting_layers":{{PlayerLayers.Replace("Back", "Rear")}}}""";
        using var loaded = DisunityHost.LoadScene(plan, new SceneLoadOptions(typeof(SortingLayerTests).Assembly, _ => []));
        Expect.Equal("Rear", SortingLayer.IDToName(7), "a loaded plan replaces the table");
        Expect.Equal("", SortingLayer.IDToName(8), "and nothing else");
    }

    private static void PlanWithoutDefault()
    {
        Register(("Units", 5), ("Top", 6));
        Expect.Sequence(["Default", "Units", "Top"], SortingLayer.layers.Select(l => l.name), "Default is added first");
        Expect.Sequence([0, 1, 2], SortingLayer.layers.Select(l => l.value), "values from the added Default");
        Register(("Units", 5));
        Register();
        Expect.Equal(1, SortingLayer.layers.Length, "an empty list leaves Default alone");
        Register(("Back", 3));
        var plan = """{"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"objects":[]}""";
        using var loaded = DisunityHost.LoadScene(plan, new SceneLoadOptions(typeof(SortingLayerTests).Assembly, _ => []));
        Expect.Equal(3, SortingLayer.NameToID("Back"), "a plan without sorting layers keeps the table");
        Expect.Throws<ArgumentException>(() => Register(("A", 1), ("B", 1)), "a duplicate id");
        Expect.Throws<ArgumentException>(() => Register(("A", 1), ("A", 2)), "a duplicate name");
        Expect.Equal(3, SortingLayer.NameToID("Back"), "a rejected list changes nothing");
    }

    private static void RoundTrips()
    {
        Register(("Back", 7), ("Default", 0), ("Units", -5));
        var owner = new GameObject("layered");
        var sprite = owner.AddComponent<SpriteRenderer>();
        var group = owner.AddComponent<SortingGroup>();
        var canvas = new GameObject("canvas").AddComponent<Canvas>();
        sprite.sortingLayerName = "Units";
        group.sortingLayerName = "Back";
        canvas.sortingLayerName = "Units";
        Expect.Equal(-5, sprite.sortingLayerID, "renderer id");
        Expect.Equal(7, group.sortingLayerID, "group id");
        Expect.Equal(-5, canvas.sortingLayerID, "canvas id");
        Expect.That(sprite.sortingLayerName == "Units" && group.sortingLayerName == "Back" && canvas.sortingLayerName == "Units", "names read back");
        sprite.sortingLayerID = 0;
        Expect.Equal("Default", sprite.sortingLayerName, "an id gives its name");
    }

    private static List<string> Warnings(Action action)
    {
        var seen = new List<string>();
        void Collect(string condition, string stackTrace, LogType type) { if (type == LogType.Warning) seen.Add(condition); }
        Application.logMessageReceivedThreaded += Collect;
        try { action(); }
        finally { Application.logMessageReceivedThreaded -= Collect; }
        return seen;
    }

    private static void UnregisteredNames()
    {
        Register(("Units", 5));
        var renderer = new GameObject("r").AddComponent<SpriteRenderer>();
        var group = renderer.gameObject.AddComponent<SortingGroup>();
        renderer.sortingLayerName = "Units";
        var warnings = Warnings(() =>
        {
            renderer.sortingLayerName = "Missing";
            renderer.sortingLayerName = "Missing";
            group.sortingLayerName = "Missing";
            renderer.sortingLayerName = "Other";
        });
        Expect.Equal(5, renderer.sortingLayerID, "the renderer keeps its layer");
        Expect.Equal(0, group.sortingLayerID, "the group keeps its layer");
        Expect.Equal(2, warnings.Count, $"one warning per name: {string.Join("|", warnings)}");
        Expect.That(warnings[0].Contains("Missing") && warnings[1].Contains("Other"), "the warnings name the layers");
        renderer.sortingLayerID = 99;
        Expect.Equal(99, renderer.sortingLayerID, "an unregistered id is stored");
        Expect.Equal("", renderer.sortingLayerName, "and has no name");
        Register(("Units", 5));
        Expect.Equal(1, Warnings(() => renderer.sortingLayerName = "Missing").Count, "a new table warns again");
    }

    private static void CanvasLoads()
    {
        var plan = $$"""
            {"scene":"level0","assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[],"differences":[],"sorting_layers":{{PlayerLayers}},"objects":[
            {"name":"Ui","tag":"Untagged","layer":5,"active":true,"parent":null,"template":false,"position":[0,0,0],"rotation":[0,0,0,1],"scale":[1,1,1],
             "rect":{"anchor_min":[0,0],"anchor_max":[1,1],"pivot":[0.5,0.5],"size_delta":[0,0],"anchored_position":[0,0]},"components":[
            {"type":"Canvas","render_mode":"ScreenSpaceOverlay","camera":null,"plane_distance":100,"pixel_perfect":false,"sorting_layer_id":9,"sorting_order":3,"target_display":0}]}]}
            """;
        using var loaded = DisunityHost.LoadScene(plan, new SceneLoadOptions(typeof(SortingLayerTests).Assembly, _ => []));
        var canvas = loaded.Objects[0].GetComponent<Canvas>()!;
        Expect.That(canvas.sortingLayerID == 9 && canvas.sortingOrder == 3 && canvas.sortingLayerName == "Top", "the canvas layer, order and name");
    }
}
