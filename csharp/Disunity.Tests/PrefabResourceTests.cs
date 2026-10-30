using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.SceneManagement;

/// <summary>
/// Prefabs a player's resource plan holds: <c>Resources.Load</c> returns the template root of a
/// prefab, <c>Instantiate</c> clones it into the active scene, a component type answers with the
/// root's component, and a prefab that did not plan throws naming itself.
/// </summary>
internal static class PrefabResourceTests
{
    private static readonly List<string> Log = new();

    public sealed class Crate : MonoBehaviour
    {
        public int hits = -1;
        public Transform? lid = null;
        public TextAsset? notes = null;
        private void Awake() => Log.Add($"awake {name} hits={hits} own-lid={lid?.parent == transform} notes={notes?.name}");
        private void Start() => Log.Add($"start {name}");
    }

    public sealed class Holder : MonoBehaviour
    {
        public GameObject? crate = null;
    }

    public static void Run()
    {
        PlanHarness.WithPlan(WritePlan, () =>
        {
            DisunityHost.LoadScene("""{"scene":"main","objects":[],"assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[]}""",
                new SceneLoadOptions(typeof(PrefabResourceTests).Assembly, _ => []));
            try
            {
                LoadReturnsTheTemplateRootOnce();
                InstantiateClonesIntoTheActiveScene();
                InactiveRootsCloneInactive();
                ComponentTypesAnswerWithTheRootsComponent();
                PrefabsReferencePrefabs();
                LoadAllFiltersByFolderAndType();
                FailedPrefabsThrowNamingThemselves();
            }
            finally { Log.Clear(); }
        });
        Console.WriteLine("PASS: resource prefabs load as templates, clone into the active scene and name their failures");
    }

    private static JsonObject Obj(string name, int? parent, bool active, params JsonNode[] components) => new()
    {
        ["name"] = name, ["tag"] = "Untagged", ["layer"] = 0, ["active"] = active, ["parent"] = parent, ["template"] = true,
        ["position"] = new JsonArray(0, 0, 0), ["rotation"] = new JsonArray(0, 0, 0, 1), ["scale"] = new JsonArray(1, 1, 1),
        ["rect"] = null, ["components"] = new JsonArray(components),
    };

    private static JsonObject ScriptComponent(Type type, JsonObject fields) => new()
    {
        ["type"] = "Script", ["script"] = "", ["class"] = type.FullName!.Replace('+', '/'),
        ["assembly"] = type.Assembly.GetName().Name, ["enabled"] = true, ["fields"] = fields,
    };

    private static JsonObject PrefabPlan(string name, params JsonObject[] objects) => new()
    {
        ["scene"] = name, ["objects"] = new JsonArray(objects), ["assets"] = new JsonArray(), ["sprites"] = new JsonArray(),
        ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
    };

    private static string WritePlan(string directory)
    {
        var data = Path.Combine(directory, "resources.assets");
        File.WriteAllBytes(data, "notes"u8.ToArray());
        Directory.CreateDirectory(Path.Combine(directory, "prefabs"));
        var crate = ScriptComponent(typeof(Crate), new JsonObject
        {
            ["hits"] = "3", ["lid"] = new JsonObject { ["$transform"] = 1 }, ["notes"] = new JsonObject { ["$asset"] = 0 },
        });
        var holder = ScriptComponent(typeof(Holder), new JsonObject { ["crate"] = new JsonObject { ["$asset"] = 1 } });
        Save(directory, 1, PrefabPlan("Crate", Obj("Crate", null, true, crate), Obj("Lid", 0, true)));
        Save(directory, 2, PrefabPlan("Dormant", Obj("Dormant", null, false)));
        Save(directory, 4, PrefabPlan("Holder", Obj("Holder", null, true, holder)));
        var assets = new JsonArray
        {
            new JsonObject { ["type"] = "TextAsset", ["name"] = "notes", ["file"] = data, ["offset"] = 0, ["length"] = 5 },
            Prefab("Crate", 1), Prefab("Dormant", 2),
            new JsonObject { ["type"] = "UnplannedPrefab", ["name"] = "Broken", ["error"] = "Broken: script Game:Nope: no layout" },
            Prefab("Holder", 4),
        };
        var entries = new JsonArray
        {
            Entry("prefabs/crate", "GameObject", 1), Entry("prefabs/dormant", "GameObject", 2),
            Entry("broken/prefab", "GameObject", 3), Entry("prefabs/holder", "GameObject", 4), Entry("text/notes", "TextAsset", 0),
        };
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static JsonObject Prefab(string name, int index) => new() { ["type"] = "Prefab", ["name"] = name, ["plan"] = $"prefabs/{index}.json" };

    private static JsonObject Entry(string path, string type, int asset) => new() { ["path"] = path, ["class"] = type, ["asset"] = asset };

    private static void Save(string directory, int index, JsonObject plan) =>
        File.WriteAllText(Path.Combine(directory, "prefabs", $"{index}.json"), plan.ToJsonString());

    private static void LoadReturnsTheTemplateRootOnce()
    {
        var prefab = Resources.Load<GameObject>("Prefabs/Crate");
        Require(prefab is { name: "Crate" }, "Load<GameObject> returns the prefab's root");
        Require(ReferenceEquals(Resources.Load("prefabs/crate/"), prefab) && ReferenceEquals(Resources.Load("prefabs/crate", typeof(GameObject)), prefab),
            "every spelling of the load returns the same object");
        Require(!prefab!.activeInHierarchy && !prefab.scene.IsValid(), "a prefab is inactive and belongs to no scene");
        Require(prefab.transform.childCount == 1 && prefab.transform.GetChild(0).name == "Lid", "the prefab keeps its children");
        Require(UnityEngine.Object.FindObjectsByType<Crate>(FindObjectsSortMode.None).Length == 0, "searches skip prefab templates");
        var crate = prefab.GetComponent<Crate>()!;
        Require(crate.hits == 3 && ReferenceEquals(crate.lid, prefab.transform.GetChild(0)) && crate.notes?.text == "notes",
            "the template's fields bind to its own child and to resource assets");
        Require(Log.Count == 0, $"a template never wakes: {string.Join(", ", Log)}");
    }

    private static void InstantiateClonesIntoTheActiveScene()
    {
        var prefab = Resources.Load<GameObject>("prefabs/crate")!;
        var clone = UnityEngine.Object.Instantiate(prefab);
        Require(clone.activeInHierarchy && clone.scene == SceneManager.GetActiveScene(), "the clone is active in the active scene");
        var crate = clone.GetComponent<Crate>()!;
        Require(Log.SequenceEqual(["awake Crate(Clone) hits=3 own-lid=True notes=notes"]), $"the clone wakes: {string.Join(", ", Log)}");
        Require(!ReferenceEquals(crate.lid, prefab.GetComponent<Crate>()!.lid) && ReferenceEquals(crate.lid, clone.transform.GetChild(0)),
            "the clone's field points at the clone's own child");
        DisunityHost.Step(0.02f);
        Require(Log.SequenceEqual(["awake Crate(Clone) hits=3 own-lid=True notes=notes", "start Crate(Clone)"]), $"the clone starts: {string.Join(", ", Log)}");
        Log.Clear();
        Require(!prefab.activeInHierarchy, "instantiating leaves the prefab a template");
    }

    private static void InactiveRootsCloneInactive()
    {
        var clone = UnityEngine.Object.Instantiate(Resources.Load<GameObject>("prefabs/dormant")!);
        Require(!clone.activeSelf && clone.scene == SceneManager.GetActiveScene(), "a prefab whose root is inactive clones inactive, into the active scene");
    }

    private static void ComponentTypesAnswerWithTheRootsComponent()
    {
        var prefab = Resources.Load<GameObject>("prefabs/crate")!;
        Require(ReferenceEquals(Resources.Load<Crate>("prefabs/crate"), prefab.GetComponent<Crate>()), "a component type answers with the root's component");
        Require(Resources.Load<Holder>("prefabs/crate") is null, "a component type the root lacks finds nothing");
        Require(ReferenceEquals(Resources.Load<Transform>("prefabs/crate"), prefab.transform), "Transform answers with the root's transform");
        Require(Resources.Load<TextAsset>("prefabs/crate") is null, "an asset type the prefab is not finds nothing");
    }

    private static void PrefabsReferencePrefabs()
    {
        var holder = Resources.Load<GameObject>("prefabs/holder")!.GetComponent<Holder>()!;
        Require(ReferenceEquals(holder.crate, Resources.Load<GameObject>("prefabs/crate")), "a field naming another prefab binds that prefab's root");
    }

    private static void LoadAllFiltersByFolderAndType()
    {
        var all = Resources.LoadAll<GameObject>("prefabs");
        Require(all.Select(value => value.name).OrderBy(name => name).SequenceEqual(["Crate", "Dormant", "Holder"]), "LoadAll<GameObject> lists the prefabs of a folder");
        Require(Resources.LoadAll<Crate>("prefabs") is [{ name: "Crate" }], "LoadAll of a component type lists the roots that have one");
    }

    private static void FailedPrefabsThrowNamingThemselves()
    {
        foreach (var load in new Action[] { () => Resources.Load<GameObject>("broken/prefab"), () => Resources.LoadAll<GameObject>("broken") })
            try
            {
                load();
                throw new InvalidOperationException("prefabs: a prefab that did not plan loaded");
            }
            catch (InvalidOperationException error) when (error.Message.Contains("Broken", StringComparison.Ordinal)
                && error.Message.Contains("script Game:Nope: no layout", StringComparison.Ordinal)) { }
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException($"prefabs: {message}");
    }
}
