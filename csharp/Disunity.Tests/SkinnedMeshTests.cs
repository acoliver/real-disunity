using System.Text.Json;
using Disunity.Hosting;
using UnityEngine;

/// <summary>The skin data of a mesh: bone weights and bind poses are validated and copied like the other channels, reach the host as
/// the blend weight and blend index channels, and build from a player's resource plan.</summary>
internal static class SkinnedMeshTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            BoneWeightsAreValidatedClearedAndCopied();
            BlendChannelsReachTheHost();
            ABoneWeightWithoutAVertexIsDroppedWithTheVertices();
        }
        finally { DisunityHost.UnloadWorld(); }
        APlannedSkinnedMeshHoldsItsWeightsAndBindPoses();
    }

    private static BoneWeight Weight(int a, float wa, int b, float wb) =>
        new() { boneIndex0 = a, weight0 = wa, boneIndex1 = b, weight1 = wb };

    private static Mesh Skinned()
    {
        var mesh = new Mesh
        {
            vertices = [new Vector3(0, 0, 0), new Vector3(1, 0, 0), new Vector3(0, 1, 0)],
            triangles = [0, 1, 2],
        };
        mesh.boneWeights = [Weight(0, 1, 0, 0), Weight(0, 0.5f, 1, 0.5f), Weight(2, 0.25f, 1, 0.75f)];
        mesh.bindposes = [Matrix4x4.identity, Matrix4x4.Translate(new Vector3(1, 2, 3))];
        return mesh;
    }

    private static void BoneWeightsAreValidatedClearedAndCopied()
    {
        var mesh = Skinned();
        Expect.Equal(Weight(0, 0.5f, 1, 0.5f), mesh.boneWeights[1], "the weights read back");
        Expect.Equal(2, mesh.bindposes.Length, "the bind poses read back");
        Expect.That(mesh.bindposes[1].GetColumn(3) == new Vector4(1, 2, 3, 1), "a bind pose keeps its translation");
        mesh.boneWeights[0] = Weight(9, 1, 0, 0);
        Expect.Equal(0, mesh.boneWeights[0].boneIndex0, "the getter returns a copy");
        Expect.Throws<ArgumentException>(() => mesh.boneWeights = [new BoneWeight()], "weights that do not match the vertices");
        var copy = UnityEngine.Object.Instantiate(mesh);
        Expect.Equal(3, copy.boneWeights.Length, "a copy keeps the weights");
        Expect.Equal(2, copy.bindposes.Length, "a copy keeps the bind poses");
        Expect.That(Weight(0, 0.5f, 1, 0.5f) == copy.boneWeights[1] && Weight(0, 1, 0, 0) != copy.boneWeights[1], "weights compare by value");
        mesh.Clear();
        Expect.That(mesh.boneWeights.Length == 0 && mesh.bindposes.Length == 0, "Clear empties the skin");
        Expect.Equal(3, copy.boneWeights.Length, "clearing the original leaves the copy");
        Console.WriteLine("PASS: bone weights match the vertices, read back as copies and survive copying; bind poses are kept beside them");
    }

    private static void BlendChannelsReachTheHost()
    {
        var stored = PlayerMeshChannelTests.Read(Skinned().RendererIdentity);
        Expect.That(stored.Has(12) && stored.Has(13), "both blend channels are stored");
        Expect.That(stored.Dimension(12) == 4 && stored.Dimension(13) == 4, "four weights and four bone indices per vertex");
        Expect.Sequence(new float[] { 1, 0, 0, 0, 0.5f, 0.5f, 0, 0, 0.25f, 0.75f, 0, 0 }, stored.Values(12), "the weights are stored in bone order of the weight record");
        Expect.Sequence(new float[] { 0, 0, 0, 0, 0, 1, 0, 0, 2, 1, 0, 0 }, stored.Values(13), "the bone indices are stored as floats");
        var plain = new Mesh { vertices = [Vector3.zero, Vector3.right, Vector3.up], triangles = [0, 1, 2] };
        var without = PlayerMeshChannelTests.Read(plain.RendererIdentity);
        Expect.That(!without.Has(12) && !without.Has(13), "a mesh without weights stores no blend channels");
        Console.WriteLine("PASS: a skinned mesh stores its weights and bone indices as channels 12 and 13");
    }

    private static void ABoneWeightWithoutAVertexIsDroppedWithTheVertices()
    {
        var mesh = new Mesh { vertices = [Vector3.zero, Vector3.right, Vector3.up] };
        mesh.boneWeights = [Weight(0, 1, 0, 0), Weight(0, 1, 0, 0), Weight(0, 1, 0, 0)];
        mesh.bindposes = [Matrix4x4.identity];
        mesh.vertices = [Vector3.zero, Vector3.one];
        Expect.Equal(0, mesh.boneWeights.Length, "weights that no longer match the vertices are dropped");
        Expect.Equal(1, mesh.bindposes.Length, "bind poses are per bone and stay");
        Console.WriteLine("PASS: weights that stop matching the vertices are dropped like the other channels");
    }

    private static void APlannedSkinnedMeshHoldsItsWeightsAndBindPoses()
    {
        var plan = """
            {"type":"Mesh","name":"Body","vertex_count":3,"index_count":3,"index_format":0,"resource":"mesh.bin",
             "channels":[{"name":"position","dimension":3},{"name":"blend weights","dimension":4},{"name":"blend indices","dimension":4}],
             "sub_meshes":[{"first_index":0,"index_count":3,"topology":0,"base_vertex":0,"first_vertex":0,"vertex_count":3}],
             "bounds":{"center":[0,0,0],"extent":[1,1,0]},
             "bind_poses":[[1,0,0,0, 0,1,0,0, 0,0,1,0, 0,0,0,1],[1,0,0,5, 0,1,0,6, 0,0,1,7, 0,0,0,1]]}
            """;
        float[] positions = [0, 0, 0, 1, 0, 0, 0, 1, 0];
        float[] weights = [1, 0, 0, 0, 0.5f, 0.5f, 0, 0, 0.25f, 0.75f, 0, 0];
        float[] joints = [0, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0];
        var mesh = Build(plan, positions.Concat(weights).Concat(joints).ToArray());
        Expect.Equal(Weight(0, 0.5f, 1, 0.5f), mesh.boneWeights[1], "the planned weights and indices make the bone weights");
        Expect.Equal(Weight(1, 0.25f, 0, 0.75f), mesh.boneWeights[2], "indices follow their weights");
        Expect.Equal(2, mesh.bindposes.Length, "the planned bind poses");
        Expect.That(mesh.bindposes[1].GetColumn(3) == new Vector4(5, 6, 7, 1), "a plan's bind pose is row-major");
        var lone = plan.Replace("{\"name\":\"blend indices\",\"dimension\":4}", "{\"name\":\"uv0\",\"dimension\":2}");
        var without = Build(lone, positions.Concat(weights).Concat(new float[6]).ToArray());
        Expect.That(without.vertexCount == 3 && without.boneWeights.Length == 0 && without.bindposes.Length == 0, "weights without indices leave the mesh without a skin");
        var narrow = plan.Replace("{\"name\":\"blend indices\",\"dimension\":4}", "{\"name\":\"blend indices\",\"dimension\":2}");
        var narrowed = Build(narrow, positions.Concat(weights).Concat(new float[6]).ToArray());
        Expect.That(narrowed.vertexCount == 3 && narrowed.boneWeights.Length == 0 && narrowed.bindposes.Length == 0, "a skin channel of two values per vertex leaves the mesh without a skin");
        Console.WriteLine("PASS: a planned skinned mesh builds its bone weights and row-major bind poses, and one with an unusable skin builds without it");
    }

    private static Mesh Build(string plan, float[] floats)
    {
        var directory = Directory.CreateTempSubdirectory("disunity-skin-").FullName;
        try
        {
            using (var writer = new BinaryWriter(File.Create(Path.Combine(directory, "mesh.bin"))))
            {
                foreach (var value in floats) writer.Write(value);
                foreach (var index in new[] { 0, 1, 2 }) writer.Write(index);
            }
            using var document = JsonDocument.Parse(plan);
            return PlayerMeshes.Build(document.RootElement, directory);
        }
        finally { Directory.Delete(directory, true); }
    }
}
