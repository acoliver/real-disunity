using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>A prefab whose skinned renderer names a mesh with a skin the engine cannot use loads whole: the mesh builds without its
/// bone weights and bind poses, the renderer is left out of frames, and the object's other components and the rest of the prefab
/// are built.</summary>
internal static class SkinnedPrefabTests
{
    private const int GoodMesh = 0, BadMesh = 1, GoodPrefab = 2, BadPrefab = 3;

    public static void Run() =>
        PlanHarness.WithPlan(WritePlan, () =>
        {
            DisunityHost.LoadScene("""{"scene":"main","objects":[],"assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[]}""",
                new SceneLoadOptions(typeof(SkinnedPrefabTests).Assembly, _ => []));
            AnUnusableSkinLeavesOnlyItsRendererOut();
        });

    private static void AnUnusableSkinLeavesOnlyItsRendererOut()
    {
        var good = UnityEngine.Object.Instantiate(Resources.Load<GameObject>("prefabs/good")!);
        var goodSkin = good.GetComponent<SkinnedMeshRenderer>()!;
        Expect.That(SkinSupport.Placed(goodSkin) is not null, "a renderer whose mesh holds a usable skin is placed");

        var bad = Resources.Load<GameObject>("prefabs/bad");
        Expect.That(bad is not null, "a prefab with an unusable skin loads");
        var rig = UnityEngine.Object.Instantiate(bad!);
        var skin = rig.GetComponent<SkinnedMeshRenderer>()!;
        Expect.That(skin.sharedMesh is { vertexCount: 3 }, "the renderer keeps the mesh and its vertices");
        Expect.That(skin.sharedMesh!.boneWeights.Length == 0 && skin.sharedMesh.bindposes.Length == 0, "the mesh holds no skin");
        Expect.That(rig.GetComponent<MeshFilter>()!.sharedMesh is { vertexCount: 3 }, "the object's other component builds");
        var second = rig.transform.GetChild(0).GetComponent<SkinnedMeshRenderer>()!;
        Expect.That(SkinSupport.Placed(second) is not null, "the next object of the prefab loads and is placed");
        Expect.That(SkinSupport.Placed(skin) is null, "a renderer whose mesh holds no usable skin is left out of frames");
        Console.WriteLine("PASS: a skinned renderer with an unusable skin is left out and the rest of its prefab loads");
    }

    private static JsonArray Floats(params float[] values) => new(values.Select(v => (JsonNode)JsonValue.Create(v)!).ToArray());

    private static JsonObject Channel(string name, int dimension) => new() { ["name"] = name, ["dimension"] = dimension };

    private static JsonObject MeshAsset(string resource, params JsonObject[] skin) => new()
    {
        ["type"] = "Mesh", ["name"] = "Body", ["vertex_count"] = 3, ["index_count"] = 3, ["index_format"] = 0, ["resource"] = resource,
        ["channels"] = new JsonArray(new[] { Channel("position", 3) }.Concat(skin).Cast<JsonNode>().ToArray()),
        ["sub_meshes"] = new JsonArray(new JsonObject
        {
            ["first_index"] = 0, ["index_count"] = 3, ["topology"] = 0, ["base_vertex"] = 0, ["first_vertex"] = 0, ["vertex_count"] = 3,
        }),
        ["bounds"] = new JsonObject { ["center"] = Floats(0, 0, 0), ["extent"] = Floats(1, 1, 0) },
        ["bind_poses"] = new JsonArray(Floats(1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1)),
    };

    private static void WriteResource(string path, int skinDimension)
    {
        using var writer = new BinaryWriter(File.Create(path));
        float[] positions = [0, 0, 0, 1, 0, 0, 0, 1, 0];
        foreach (var value in positions) writer.Write(value);
        for (var vertex = 0; vertex < 3; vertex++)
            for (var slot = 0; slot < skinDimension; slot++) writer.Write(slot == 0 ? 1f : 0f);
        for (var vertex = 0; vertex < 3; vertex++)
            for (var slot = 0; slot < skinDimension; slot++) writer.Write(0f);
        foreach (var index in new[] { 0, 1, 2 }) writer.Write(index);
    }

    private static JsonObject Skinned(int mesh) => new()
    {
        ["type"] = "SkinnedMeshRenderer", ["enabled"] = true, ["receive_shadows"] = true, ["dynamic_occludee"] = true, ["static_shadow_caster"] = false,
        ["shadow_casting_mode"] = 1, ["motion_vector_mode"] = 1, ["light_probe_usage"] = 1, ["reflection_probe_usage"] = 1,
        ["rendering_layer_mask"] = 1, ["renderer_priority"] = 0, ["sorting_layer_id"] = 0, ["sorting_order"] = 0, ["materials"] = new JsonArray(),
        ["quality"] = 0, ["update_when_offscreen"] = false, ["mesh"] = mesh, ["bones"] = new JsonArray(0), ["root_bone"] = 0,
    };

    private static JsonObject Obj(string name, int? parent, params JsonNode[] components) => new()
    {
        ["name"] = name, ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = parent, ["template"] = true,
        ["position"] = Floats(0, 0, 0), ["rotation"] = Floats(0, 0, 0, 1), ["scale"] = Floats(1, 1, 1), ["rect"] = null,
        ["components"] = new JsonArray(components),
    };

    private static void SavePrefab(string directory, string file, int mesh)
    {
        var filter = new JsonObject { ["type"] = "MeshFilter", ["mesh"] = mesh };
        var plan = new JsonObject
        {
            ["scene"] = file, ["objects"] = new JsonArray(Obj("Rig", null, Skinned(mesh), filter), Obj("Second", 0, Skinned(GoodMesh))),
            ["assets"] = new JsonArray(), ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
        };
        File.WriteAllText(Path.Combine(directory, file + ".json"), plan.ToJsonString());
    }

    private static string WritePlan(string directory)
    {
        Directory.CreateDirectory(Path.Combine(directory, "resources"));
        WriteResource(Path.Combine(directory, "resources", "0.bin"), 4);
        WriteResource(Path.Combine(directory, "resources", "1.bin"), 2);
        SavePrefab(directory, "good", GoodMesh);
        SavePrefab(directory, "bad", BadMesh);
        var assets = new JsonArray
        {
            MeshAsset("resources/0.bin", Channel("blend weights", 4), Channel("blend indices", 4)),
            MeshAsset("resources/1.bin", Channel("blend weights", 2), Channel("blend indices", 2)),
            new JsonObject { ["type"] = "Prefab", ["name"] = "Good", ["plan"] = "good.json" },
            new JsonObject { ["type"] = "Prefab", ["name"] = "Bad", ["plan"] = "bad.json" },
        };
        var entries = new JsonArray(
            new JsonObject { ["path"] = "prefabs/good", ["class"] = "GameObject", ["asset"] = GoodPrefab },
            new JsonObject { ["path"] = "prefabs/bad", ["class"] = "GameObject", ["asset"] = BadPrefab });
        return PlanHarness.WriteJson(directory, entries, assets);
    }
}
