using UnityEngine;

internal static partial class MeshTests
{
    private static void Near(Vector3 expected, Vector3 actual, string what) =>
        Expect.That((expected - actual).magnitude < 1e-4f, $"{what}: expected {expected}, got {actual}");

    private static void Near(Vector4 expected, Vector4 actual, string what) =>
        Expect.That((expected - actual).magnitude < 1e-4f, $"{what}: expected {expected}, got {actual}");

    private static void NormalsOfAQuad()
    {
        var mesh = QuadMesh();
        mesh.RecalculateNormals();
        Expect.Equal(4, mesh.normals.Length, "one normal per vertex");
        foreach (var normal in mesh.normals) Near(new Vector3(0, 0, -1), normal, "a quad in the XY plane wound clockwise from -z faces -z");
        mesh.triangles = [0, 2, 1, 0, 3, 2];
        mesh.RecalculateNormals();
        foreach (var normal in mesh.normals) Near(new Vector3(0, 0, 1), normal, "reversing the winding flips the normals");
        var empty = new Mesh();
        empty.RecalculateNormals();
        Expect.Equal(0, empty.normals.Length, "an empty mesh gets no normals");
    }

    private static void NormalsAreAreaWeighted()
    {
        var mesh = new Mesh
        {
            vertices = [Vector3.zero, new(0, 2, 0), new(2, 0, 0), new(0, 0, 1), new(0, 1, 0), new(5, 5, 5)],
        };
        mesh.subMeshCount = 3;
        mesh.SetTriangles(new[] { 0, 1, 2 }, 0);
        mesh.SetTriangles(new[] { 0, 3, 4 }, 1);
        mesh.SetIndices(new[] { 0, 5 }, MeshTopology.Lines, 2);
        mesh.normals = new Vector3[6];
        mesh.RecalculateNormals();
        var shared = new Vector3(-1, 0, -4).normalized;
        Near(shared, mesh.normals[0], "a shared vertex weights each face by its area");
        Near(new Vector3(0, 0, -1), mesh.normals[1], "a vertex of the large triangle only");
        Near(new Vector3(-1, 0, 0), mesh.normals[3], "a vertex of the small triangle only");
        Near(Vector3.zero, mesh.normals[5], "a vertex without a triangle gets (0, 0, 0)");
        var shifted = new Mesh { vertices = [new(9, 9, 9), Vector3.zero, new(0, 1, 0), new(1, 0, 0)] };
        shifted.SetTriangles(new[] { 0, 1, 2 }, 0, true, 1);
        shifted.RecalculateNormals();
        Near(new Vector3(0, 0, -1), shifted.normals[2], "the base vertex moves the triangle to its vertices");
        Near(Vector3.zero, shifted.normals[0], "a vertex before the base vertex has no triangle");
    }

    private static Mesh QuadWithUvs(bool mirror)
    {
        var mesh = QuadMesh();
        mesh.RecalculateNormals();
        mesh.uv = mirror
            ? [new(1, 0), new(1, 1), new(0, 1), new(0, 0)]
            : [new(0, 0), new(0, 1), new(1, 1), new(1, 0)];
        return mesh;
    }

    private static void TangentsOfAQuad()
    {
        var mesh = QuadWithUvs(false);
        mesh.RecalculateTangents();
        Expect.Equal(4, mesh.tangents.Length, "one tangent per vertex");
        foreach (var tangent in mesh.tangents) Near(new Vector4(1, 0, 0, -1), tangent, "u grows along +x with the normal toward -z");
        var mirrored = QuadWithUvs(true);
        mirrored.RecalculateTangents();
        foreach (var tangent in mirrored.tangents) Near(new Vector4(-1, 0, 0, 1), tangent, "mirrored u flips the tangent and the handedness");
        var tilted = QuadWithUvs(false);
        var lean = new Vector3(1, 0, -1).normalized;
        tilted.normals = new[] { lean, lean, lean, lean };
        tilted.RecalculateTangents();
        foreach (var tangent in tilted.tangents)
        {
            Expect.Near(0f, Vector3.Dot(lean, tangent), "the tangent is orthogonal to the normal", 1e-4f);
            Expect.Near(1f, ((Vector3)tangent).magnitude, "the tangent is a unit vector", 1e-4f);
            Expect.Near(1f, MathF.Abs(tangent.w), "w is the handedness", 1e-6f);
        }
        var flat = QuadMesh();
        flat.RecalculateNormals();
        flat.uv = new Vector2[4];
        flat.RecalculateTangents();
        foreach (var tangent in flat.tangents)
        {
            Expect.Near(0f, Vector3.Dot(new Vector3(0, 0, -1), tangent), "a vertex without usable uvs still gets a tangent perpendicular to its normal", 1e-4f);
            Expect.Near(1f, ((Vector3)tangent).magnitude, "the fallback tangent is a unit vector", 1e-4f);
            Expect.Equal(1f, tangent.w, "the fallback handedness is +1");
        }
    }

    private static void TangentsNeedNormalsAndUvs()
    {
        var noNormals = QuadMesh();
        noNormals.uv = new Vector2[4];
        var first = Expect.Catch<InvalidOperationException>(noNormals.RecalculateTangents, "tangents without normals");
        Expect.That(first.Message.Contains("normals", StringComparison.Ordinal), $"the message names the normals: {first.Message}");
        var noUvs = QuadMesh();
        noUvs.RecalculateNormals();
        var second = Expect.Catch<InvalidOperationException>(noUvs.RecalculateTangents, "tangents without uv");
        Expect.That(second.Message.Contains("uv", StringComparison.Ordinal), $"the message names the uv channel: {second.Message}");
        Expect.Equal(0, noUvs.tangents.Length, "a refused recalculation sets no tangents");
    }

    private static void ClearEmptiesTheMesh()
    {
        var mesh = QuadWithUvs(false);
        mesh.colors = new Color[4];
        mesh.subMeshCount = 3;
        mesh.bounds = new Bounds(Vector3.one, Vector3.one);
        mesh.Clear();
        Expect.Equal(0, mesh.vertexCount, "Clear removes the positions");
        Expect.Equal(0, mesh.normals.Length + mesh.uv.Length + mesh.colors.Length + mesh.tangents.Length, "Clear removes the channels");
        Expect.Equal(1, mesh.subMeshCount, "Clear leaves one sub-mesh");
        Expect.Equal(0, mesh.GetIndexCount(0), "Clear leaves it without indices");
        Expect.Equal(default, mesh.bounds, "Clear zeroes the bounds");
        mesh.vertices = QuadPositions();
        mesh.uv = new Vector2[4];
        mesh.Clear(false);
        Expect.Equal(0, mesh.vertexCount + mesh.uv.Length + mesh.GetIndexCount(0), "Clear(bool) empties as well");
        var snapshot = mesh.Snapshot();
        Expect.Equal(0, snapshot.UvDimensions[0], "Clear resets the uv dimension");
    }

    private static void UnreadableAfterUpload()
    {
        var mesh = QuadWithUvs(false);
        mesh.UploadMeshData(false);
        Expect.That(mesh.isReadable, "UploadMeshData(false) keeps the mesh readable");
        mesh.UploadMeshData(true);
        Expect.That(!mesh.isReadable, "UploadMeshData(true) makes it unreadable");
        var blocked = new (string What, Action Act)[]
        {
            ("vertices get", () => _ = mesh.vertices),
            ("vertices set", () => mesh.vertices = QuadPositions()),
            ("normals get", () => _ = mesh.normals),
            ("uv set", () => mesh.uv = new Vector2[4]),
            ("colors32 get", () => _ = mesh.colors32),
            ("GetUVs", () => mesh.GetUVs(0, new List<Vector2>())),
            ("triangles get", () => _ = mesh.triangles),
            ("triangles set", () => mesh.triangles = [0, 1, 2]),
            ("SetTriangles", () => mesh.SetTriangles(new[] { 0, 1, 2 }, 0)),
            ("GetIndices", () => mesh.GetIndices(0)),
            ("GetTriangles", () => mesh.GetTriangles(0)),
            ("subMeshCount set", () => mesh.subMeshCount = 2),
            ("RecalculateBounds", mesh.RecalculateBounds),
            ("RecalculateNormals", mesh.RecalculateNormals),
            ("RecalculateTangents", mesh.RecalculateTangents),
            ("Clear", mesh.Clear),
        };
        foreach (var (what, act) in blocked)
        {
            var error = Expect.Catch<InvalidOperationException>(act, $"{what} on an unreadable mesh");
            Expect.That(error.Message.Contains("mesh is not readable", StringComparison.Ordinal), $"{what}: {error.Message}");
        }
        mesh.bounds = new Bounds(Vector3.one, Vector3.one);
        Expect.Equal(new Bounds(Vector3.one, Vector3.one), mesh.bounds, "bounds still work");
        Expect.Equal(4, mesh.vertexCount, "vertexCount still works");
        Expect.Equal(1, mesh.subMeshCount, "subMeshCount still works");
        Expect.Equal(6, mesh.GetIndexCount(0) + mesh.GetIndexStart(0) + mesh.GetBaseVertex(0), "sub-mesh queries still work");
        Expect.Equal(MeshTopology.Triangles, mesh.GetTopology(0), "GetTopology still works");
        Expect.Equal(4, mesh.Snapshot().Positions.Length, "the renderer still reads the data");
    }

    private static void OptimizeKeepsData()
    {
        var mesh = QuadWithUvs(false);
        var before = mesh.Snapshot();
        mesh.MarkDynamic();
        mesh.Optimize();
        mesh.OptimizeIndexBuffers();
        mesh.OptimizeReorderVertexBuffer();
        var after = mesh.Snapshot();
        Expect.Sequence(before.Positions, after.Positions, "positions unchanged by Optimize");
        Expect.Sequence(before.SubMeshes[0].Indices, after.SubMeshes[0].Indices, "indices unchanged by Optimize");
        Expect.Sequence(before.Normals, after.Normals, "normals unchanged by Optimize");
    }

    private static void SnapshotIsACopy()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        mesh.uv = new Vector2[4];
        mesh.SetUVs(1, new List<Vector4>(new Vector4[4]));
        mesh.SetUVs(2, new Vector3[4]);
        mesh.SetUVs(3, new Vector3[4]);
        mesh.SetUVs(3, new Vector3[0]);
        mesh.subMeshCount = 2;
        mesh.SetIndices(new[] { 1, 2 }, MeshTopology.Lines, 1, false, 1);
        var snapshot = mesh.Snapshot();
        Expect.Sequence(new[] { 2, 4, 3, 0 }, snapshot.UvDimensions.Take(4), "the snapshot records the uv dimensions, and an emptied channel has none");
        Expect.Equal(MeshTopology.Lines, snapshot.SubMeshes[1].Topology, "the snapshot holds the topology");
        Expect.Equal(1, snapshot.SubMeshes[1].BaseVertex, "the snapshot holds the base vertex");
        snapshot.Positions[0] = new Vector3(8, 8, 8);
        Expect.Equal(Vector3.zero, mesh.vertices[0], "changing the snapshot does not change the mesh");
        Expect.Equal(IndexFormatOf(mesh), snapshot.IndexFormat, "the snapshot holds the index format");
    }

    private static UnityEngine.Rendering.IndexFormat IndexFormatOf(Mesh mesh) => mesh.indexFormat;
}
