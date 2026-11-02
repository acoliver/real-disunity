using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Rendering;

/// <summary>Meshes a player's resource plan holds: each plans once with its channels, sub-meshes and bounds, every
/// <c>MeshFilter</c> that names one shares the object, and the caller-list getters fill a list after clearing it.</summary>
internal static class PlayerMeshTests
{
    public static void Run()
    {
        PlanHarness.WithPlan(WritePlan, () =>
        {
            DisunityHost.LoadScene("""{"scene":"main","objects":[],"assets":[],"sprites":[],"textures":[],"fonts":[],"axes":[]}""",
                new SceneLoadOptions(typeof(PlayerMeshTests).Assembly, _ => []));
            AMeshBuildsWithItsChannelsAndSubMeshes();
            FiltersThatNameOneMeshShareIt();
            ClonesShareTheMeshAndFilterMeshCopiesIt();
            ListGettersClearTheCallersListAndFillIt();
            ABigIndexFormatAndAMissingMeshArePlanned();
        });
        LaterUvChannelsAndTrailingBytes();
    }

    /// <summary>Builds <paramref name="asset"/> from a resource holding <paramref name="floats"/> then the indices.</summary>
    private static Mesh BuildDirect(JsonObject asset, float[] floats, int[] indices, int trailing)
    {
        var directory = Directory.CreateTempSubdirectory("disunity-mesh-").FullName;
        try
        {
            using (var writer = new BinaryWriter(File.Create(Path.Combine(directory, "mesh.bin"))))
            {
                foreach (var value in floats) writer.Write(value);
                foreach (var index in indices) writer.Write(index);
                writer.Write(new byte[trailing]);
            }
            using var plan = System.Text.Json.JsonDocument.Parse(asset.ToJsonString());
            return PlayerMeshes.Build(plan.RootElement, directory);
        }
        finally { Directory.Delete(directory, true); }
    }

    private static void LaterUvChannelsAndTrailingBytes()
    {
        JsonObject Asset() => new()
        {
            ["type"] = "Mesh", ["name"] = "Second", ["vertex_count"] = 3, ["index_count"] = 3, ["index_format"] = 0, ["resource"] = "mesh.bin",
            ["channels"] = new JsonArray(Channel("position", 3), Channel("uv2", 2)),
            ["sub_meshes"] = new JsonArray(new JsonObject
            {
                ["first_index"] = 0, ["index_count"] = 3, ["topology"] = 0, ["base_vertex"] = 0, ["first_vertex"] = 0, ["vertex_count"] = 3,
            }),
            ["bounds"] = new JsonObject { ["center"] = Floats(0, 0, 0), ["extent"] = Floats(1, 1, 0) },
        };
        float[] floats = [0, 0, 0, 1, 0, 0, 0, 1, 0, 0.25f, 0.5f, 0.75f, 1, 0, 0];
        var mesh = BuildDirect(Asset(), floats, [0, 1, 2], 0);
        var third = new List<Vector2>();
        mesh.GetUVs(2, third);
        Expect.Sequence(new[] { new Vector2(0.25f, 0.5f), new(0.75f, 1), new(0, 0) }, third, "a uv2 channel lands in the third uv set");
        Expect.Equal(0, mesh.uv.Length, "the first uv set stays empty");
        Expect.Throws<InvalidDataException>(() => BuildDirect(Asset(), floats, [0, 1, 2], 4), "bytes after the indices");
        Console.WriteLine("PASS: a later uv channel lands in its own set and extra resource bytes are refused");
    }

    private static JsonArray Floats(params float[] values) => new(values.Select(v => (JsonNode)JsonValue.Create(v)!).ToArray());

    private static JsonObject Channel(string name, int dimension) => new() { ["name"] = name, ["dimension"] = dimension };

    private static JsonObject Sub(int first, int count, int baseVertex) => new()
    {
        ["first_index"] = first, ["index_count"] = count, ["topology"] = 0, ["base_vertex"] = baseVertex, ["first_vertex"] = 0, ["vertex_count"] = 4,
    };

    private static JsonObject MeshAsset(string resource, int indexFormat) => new()
    {
        ["type"] = "Mesh", ["name"] = "Hex", ["vertex_count"] = 4, ["index_count"] = 6, ["index_format"] = indexFormat, ["resource"] = resource,
        ["channels"] = new JsonArray(Channel("position", 3), Channel("uv0", 2)),
        ["sub_meshes"] = new JsonArray(Sub(0, 3, 0), Sub(3, 3, 1)),
        ["bounds"] = new JsonObject { ["center"] = Floats(0.5f, 0.5f, 0), ["extent"] = Floats(0.5f, 0.5f, 0.25f) },
    };

    private static void WriteResource(string path)
    {
        using var writer = new BinaryWriter(File.Create(path));
        float[] positions = [0, 0, 0, 1, 0, 0, 0, 1, 0, 1, 1, 0];
        float[] uvs = [0, 0, 1, 0, 0, 1, 1, 1];
        foreach (var value in positions.Concat(uvs)) writer.Write(value);
        foreach (var index in new[] { 0, 1, 2, 0, 1, 2 }) writer.Write(index);
    }

    private static JsonObject Obj(string name, params JsonNode[] components) => new()
    {
        ["name"] = name, ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = null, ["template"] = true,
        ["position"] = Floats(0, 0, 0), ["rotation"] = Floats(0, 0, 0, 1), ["scale"] = Floats(1, 1, 1), ["rect"] = null,
        ["components"] = new JsonArray(components),
    };

    private static void SavePrefab(string directory, string file, string name, JsonNode? mesh)
    {
        var plan = new JsonObject
        {
            ["scene"] = name, ["objects"] = new JsonArray(Obj(name, new JsonObject { ["type"] = "MeshFilter", ["mesh"] = mesh })),
            ["assets"] = new JsonArray(), ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
        };
        File.WriteAllText(Path.Combine(directory, file), plan.ToJsonString());
    }

    private static string WritePlan(string directory)
    {
        Directory.CreateDirectory(Path.Combine(directory, "resources"));
        WriteResource(Path.Combine(directory, "resources", "0.bin"));
        File.Copy(Path.Combine(directory, "resources", "0.bin"), Path.Combine(directory, "resources", "3.bin"));
        SavePrefab(directory, "rock.json", "Rock", 0);
        SavePrefab(directory, "twin.json", "Twin", 0);
        SavePrefab(directory, "bare.json", "Bare", null);
        var assets = new JsonArray
        {
            MeshAsset("resources/0.bin", 0),
            new JsonObject { ["type"] = "Prefab", ["name"] = "Rock", ["plan"] = "rock.json" },
            new JsonObject { ["type"] = "Prefab", ["name"] = "Twin", ["plan"] = "twin.json" },
            MeshAsset("resources/3.bin", 1),
            new JsonObject { ["type"] = "Prefab", ["name"] = "Bare", ["plan"] = "bare.json" },
        };
        string[] paths = ["meshes/hex", "prefabs/rock", "prefabs/twin", "meshes/wide", "prefabs/bare"];
        string[] classes = ["Mesh", "GameObject", "GameObject", "Mesh", "GameObject"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static void AMeshBuildsWithItsChannelsAndSubMeshes()
    {
        var mesh = Resources.Load<Mesh>("meshes/hex")!;
        Expect.Equal("Hex", mesh.name, "the mesh has its planned name");
        Expect.Equal(4, mesh.vertexCount, "the mesh has its vertices");
        Expect.That(mesh.isReadable, "a planned mesh is readable");
        Expect.Sequence(new[] { new Vector3(0, 0, 0), new(1, 0, 0), new(0, 1, 0), new(1, 1, 0) }, mesh.vertices, "the positions");
        Expect.Sequence(new[] { new Vector2(0, 0), new(1, 0), new(0, 1), new(1, 1) }, mesh.uv, "the first uv channel");
        Expect.Equal(0, mesh.normals.Length, "a channel the plan does not name is empty");
        Expect.Equal(2, mesh.subMeshCount, "the mesh has its sub-meshes");
        Expect.Sequence(new[] { 0, 1, 2 }, mesh.GetIndices(0), "sub-mesh 0 indices");
        Expect.Sequence(new[] { 1, 2, 3 }, mesh.GetIndices(1), "sub-mesh 1 indices with its base vertex");
        Expect.Sequence(new[] { 0, 1, 2 }, mesh.GetIndices(1, false), "sub-mesh 1 indices without its base vertex");
        Expect.Equal(1u, mesh.GetBaseVertex(1), "sub-mesh 1 base vertex");
        Expect.Equal(MeshTopology.Triangles, mesh.GetTopology(1), "sub-mesh topology");
        Expect.Equal(new Vector3(0.5f, 0.5f, 0), mesh.bounds.center, "the bounds center");
        Expect.Equal(new Vector3(1, 1, 0.5f), mesh.bounds.size, "the bounds size is twice the planned extent");
        Console.WriteLine("PASS: a planned mesh has its name, vertices, uvs, sub-mesh indices, base vertex, topology and bounds");
    }

    private static void FiltersThatNameOneMeshShareIt()
    {
        var rock = Resources.Load<GameObject>("prefabs/rock")!.GetComponent<MeshFilter>()!.sharedMesh;
        var twin = Resources.Load<GameObject>("prefabs/twin")!.GetComponent<MeshFilter>()!.sharedMesh;
        Expect.That(rock is not null && ReferenceEquals(rock, twin), "two filters that name one planned mesh get one object");
        Expect.That(ReferenceEquals(rock, Resources.Load<Mesh>("meshes/hex")), "Resources.Load<Mesh> returns the filters' mesh");
        Console.WriteLine("PASS: filters that name one planned mesh share it");
    }

    private static void ClonesShareTheMeshAndFilterMeshCopiesIt()
    {
        var shared = Resources.Load<Mesh>("meshes/hex")!;
        var clone = UnityEngine.Object.Instantiate(Resources.Load<GameObject>("prefabs/rock")!);
        var filter = clone.GetComponent<MeshFilter>()!;
        Expect.That(ReferenceEquals(filter.sharedMesh, shared), "a clone's filter shares the planned mesh");
        var own = filter.mesh!;
        Expect.That(!ReferenceEquals(own, shared) && own.name == "Hex Instance" && own.vertexCount == 4, "mesh gives the filter its own copy");
        Expect.That(ReferenceEquals(shared, Resources.Load<Mesh>("meshes/hex")), "the planned mesh is unchanged");
        Console.WriteLine("PASS: clones share the planned mesh and mesh copies it on first read");
    }

    private static void ListGettersClearTheCallersListAndFillIt()
    {
        var mesh = Resources.Load<Mesh>("meshes/hex")!;
        var vertices = new List<Vector3> { new(9, 9, 9), new(8, 8, 8), new(7, 7, 7), new(6, 6, 6), new(5, 5, 5) };
        mesh.GetVertices(vertices);
        Expect.Equal(4, vertices.Count, "GetVertices clears the list before it fills it");
        var uvs = new List<Vector2> { new(9, 9) };
        mesh.GetUVs(0, uvs);
        Expect.Sequence(new[] { new Vector2(0, 0), new(1, 0), new(0, 1), new(1, 1) }, uvs, "GetUVs fills the list");
        var indices = new List<int> { 42, 43, 44, 45 };
        mesh.GetIndices(indices, 1);
        Expect.Sequence(new[] { 1, 2, 3 }, indices, "GetIndices(list, submesh) clears the list and applies the base vertex");
        Console.WriteLine("PASS: GetVertices, GetUVs and GetIndices fill the caller's list after clearing it");
    }

    private static void ABigIndexFormatAndAMissingMeshArePlanned()
    {
        Expect.Equal(IndexFormat.UInt32, Resources.Load<Mesh>("meshes/wide")!.indexFormat, "a 32-bit index format is kept");
        Expect.Equal(IndexFormat.UInt16, Resources.Load<Mesh>("meshes/hex")!.indexFormat, "a 16-bit index format is kept");
        Expect.That(Resources.Load<GameObject>("prefabs/bare")!.GetComponent<MeshFilter>()!.sharedMesh is null, "a filter without a planned mesh has none");
        Console.WriteLine("PASS: the index format is kept and a filter without a mesh stays empty");
    }
}
