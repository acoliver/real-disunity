using UnityEngine;
using UnityEngine.Rendering;

internal static partial class MeshTests
{
    private static Mesh QuadMesh()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        mesh.triangles = [0, 1, 2, 0, 2, 3];
        return mesh;
    }

    private static void SubMeshCounts()
    {
        var mesh = new Mesh();
        foreach (var name in new[] { nameof(Mesh.GetIndexCount), nameof(Mesh.GetIndexStart), nameof(Mesh.GetBaseVertex) })
            Expect.That(typeof(Mesh).GetMethod(name, [typeof(int)])!.ReturnType == typeof(uint), $"{name} returns an unsigned count, as compiled game code expects");
        Expect.Equal(1, mesh.subMeshCount, "a new mesh has one empty sub-mesh");
        Expect.Equal(0u, mesh.GetIndexCount(0), "the first sub-mesh is empty");
        mesh.vertices = QuadPositions();
        mesh.SetTriangles(new List<int> { 0, 1, 2 }, 0);
        mesh.subMeshCount = 3;
        Expect.Equal(3, mesh.subMeshCount, "subMeshCount grows");
        Expect.Equal(3u, mesh.GetIndexCount(0), "growing keeps the indices");
        Expect.Equal(0u, mesh.GetIndexCount(2), "a grown sub-mesh is empty");
        mesh.SetTriangles(new List<int> { 0, 2, 3, 3, 2, 1 }, 2);
        mesh.subMeshCount = 2;
        Expect.Equal(2, mesh.subMeshCount, "subMeshCount truncates");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.GetIndexCount(2), "a truncated sub-mesh is gone");
        mesh.subMeshCount = 1;
        Expect.Sequence(new[] { 0, 1, 2 }, mesh.triangles, "truncating keeps the first sub-mesh");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.subMeshCount = -1, "a negative count");
    }

    private static void SubMeshQueries()
    {
        var mesh = QuadMesh();
        mesh.subMeshCount = 3;
        mesh.SetTriangles(new[] { 0, 1, 2 }, 1, true, 1);
        mesh.SetTriangles(new[] { 3, 2, 1 }, 2);
        Expect.Equal(0u, mesh.GetIndexStart(0), "the first sub-mesh starts at 0");
        Expect.Equal(6u, mesh.GetIndexStart(1), "a sub-mesh starts after the indices before it");
        Expect.Equal(9u, mesh.GetIndexStart(2), "the running sum of earlier counts");
        Expect.Equal(6u, mesh.GetIndexCount(0), "GetIndexCount");
        Expect.Equal(3u, mesh.GetIndexCount(1), "GetIndexCount of the second");
        Expect.Equal(0u, mesh.GetBaseVertex(0), "base vertex defaults to 0");
        Expect.Equal(1u, mesh.GetBaseVertex(1), "GetBaseVertex");
        foreach (var index in new[] { -1, 3 })
        {
            Expect.Throws<ArgumentOutOfRangeException>(() => mesh.GetIndexStart(index), $"GetIndexStart({index})");
            Expect.Throws<ArgumentOutOfRangeException>(() => mesh.GetIndexCount(index), $"GetIndexCount({index})");
            Expect.Throws<ArgumentOutOfRangeException>(() => mesh.GetBaseVertex(index), $"GetBaseVertex({index})");
            Expect.Throws<ArgumentOutOfRangeException>(() => mesh.GetTopology(index), $"GetTopology({index})");
            Expect.Throws<ArgumentOutOfRangeException>(() => mesh.GetTriangles(index), $"GetTriangles({index})");
        }
        Expect.Sequence(new[] { 1, 2, 3 }, mesh.GetTriangles(1), "GetTriangles adds the base vertex");
        var list = new List<int> { 99 };
        mesh.GetTriangles(list, 1);
        Expect.Sequence(new[] { 1, 2, 3 }, list, "GetTriangles(List) clears the list and adds the base vertex");
        mesh.GetIndices(list, 1);
        Expect.Sequence(new[] { 1, 2, 3 }, list, "GetIndices(List, int)");
        mesh.GetIndices(list, 1, false);
        Expect.Sequence(new[] { 0, 1, 2 }, list, "GetIndices without the base vertex");
        Expect.Sequence(new[] { 0, 1, 2 }, mesh.GetIndices(1, false), "GetIndices(int, bool)");
        Expect.Sequence(new[] { 0, 1, 2, 0, 2, 3, 0, 1, 2, 3, 2, 1 }, mesh.GetIndices(0, true).Concat(mesh.GetIndices(1, false)).Concat(mesh.GetIndices(2)), "GetIndices(int)");
    }

    private static void TrianglesRules()
    {
        var mesh = QuadMesh();
        Expect.Sequence(new[] { 0, 1, 2, 0, 2, 3 }, mesh.triangles, "triangles round trip");
        var bad = new (string What, int[] Indices)[]
        {
            ("a count that is not a multiple of 3", [0, 1, 2, 3]),
            ("an index past the last vertex", [0, 1, 4]),
            ("a negative index", [0, -1, 2]),
        };
        foreach (var (what, indices) in bad)
        {
            Expect.Throws<ArgumentException>(() => mesh.triangles = indices, $"triangles with {what}");
            Expect.Throws<ArgumentException>(() => mesh.SetTriangles(indices, 0), $"SetTriangles(array) with {what}");
            Expect.Throws<ArgumentException>(() => mesh.SetTriangles(new List<int>(indices), 0, false), $"SetTriangles(List, bool) with {what}");
        }
        Expect.Sequence(new[] { 0, 1, 2, 0, 2, 3 }, mesh.triangles, "failed sets leave the indices alone");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.SetTriangles(new List<int> { 0, 1, 2 }, 1), "submesh == subMeshCount");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.SetTriangles(new[] { 0, 1, 2 }, -1), "negative submesh");
        mesh.subMeshCount = 2;
        mesh.SetTriangles(new List<int> { 3, 2, 1 }, 1, true);
        Expect.Sequence(new[] { 0, 1, 2, 0, 2, 3, 3, 2, 1 }, mesh.triangles, "triangles joins the sub-meshes");
        mesh.triangles = [0, 1, 2];
        Expect.Equal(1, mesh.subMeshCount, "setting triangles replaces all sub-meshes with one");
        mesh.triangles = null!;
        Expect.Equal(0, mesh.triangles.Length, "null triangles clear the indices");
        mesh.SetTriangles(new int[] { 0, 1, 2 }, 0, true, 0);
        mesh.SetTriangles((List<int>)null!, 0);
        Expect.Equal(0u, mesh.GetIndexCount(0), "a null list empties the sub-mesh");
        mesh.SetTriangles(new List<int> { 0, 1, 2 }, 0, false, 0);
        mesh.SetTriangles(new List<int> { 0, 1, 2 }, 0, true, 0);
        Expect.Equal(3u, mesh.GetIndexCount(0), "SetTriangles with a base vertex of 0");
    }

    private static void BaseVertexRange()
    {
        var mesh = QuadMesh();
        mesh.SetTriangles(new List<int> { 0, 1, 2 }, 0, true, 1);
        Expect.Equal(1u, mesh.GetBaseVertex(0), "SetTriangles records the base vertex");
        Expect.Throws<ArgumentException>(() => mesh.SetTriangles(new List<int> { 0, 1, 3 }, 0, true, 1), "index + baseVertex past the last vertex");
        Expect.Throws<ArgumentException>(() => mesh.SetTriangles(new[] { 0, 1, 2 }, 0, true, -1), "index + baseVertex below 0");
        Expect.Equal(1u, mesh.GetBaseVertex(0), "a failed set keeps the base vertex");
        mesh.SetTriangles(new[] { 1, 2, 3 }, 0, true, -1);
        Expect.Equal(uint.MaxValue, mesh.GetBaseVertex(0), "a negative base vertex that stays in range reads as unsigned");
        mesh.SetTriangles(new[] { 0, 1, 2 }, 0);
        Expect.Equal(0u, mesh.GetBaseVertex(0), "SetTriangles without a base vertex resets it");
    }

    private static void IndexFormatLimit()
    {
        var mesh = new Mesh { vertices = new Vector3[70000] };
        Expect.Equal(IndexFormat.UInt16, mesh.indexFormat, "the default index format");
        mesh.triangles = [0, 1, 65535];
        Expect.Throws<ArgumentException>(() => mesh.triangles = [0, 1, 65536], "an index above 65535 with UInt16");
        Expect.Throws<ArgumentException>(() => mesh.SetIndices(new[] { 65536 }, MeshTopology.Points, 0), "SetIndices above 65535 with UInt16");
        mesh.indexFormat = IndexFormat.UInt32;
        mesh.triangles = [0, 1, 69999];
        Expect.Equal(69999, mesh.triangles[2], "UInt32 holds large indices");
        Expect.Throws<InvalidOperationException>(() => mesh.indexFormat = IndexFormat.UInt16, "narrowing with a large index");
        Expect.Equal(IndexFormat.UInt32, mesh.indexFormat, "a refused narrowing keeps UInt32");
        mesh.triangles = [0, 1, 2];
        mesh.indexFormat = IndexFormat.UInt16;
        Expect.Equal(IndexFormat.UInt16, mesh.indexFormat, "narrowing with small indices");
    }

    private static void TopologiesAndSetIndices()
    {
        var mesh = new Mesh { vertices = QuadPositions() };
        mesh.subMeshCount = 5;
        Expect.Equal(MeshTopology.Triangles, mesh.GetTopology(0), "the default topology");
        mesh.SetIndices(new[] { 0, 1, 1, 2 }, MeshTopology.Lines, 0);
        mesh.SetIndices(new List<int> { 0, 1, 2, 3 }, MeshTopology.LineStrip, 1);
        mesh.SetIndices(new[] { 0, 1, 2, 3 }, MeshTopology.Quads, 2, false);
        mesh.SetIndices(new List<int> { 3, 2 }, MeshTopology.Points, 3, true);
        mesh.SetIndices(new[] { 1, 2, 3 }, MeshTopology.Triangles, 4, true, 0);
        Expect.Equal(MeshTopology.Lines, mesh.GetTopology(0), "Lines");
        Expect.Equal(MeshTopology.LineStrip, mesh.GetTopology(1), "LineStrip");
        Expect.Equal(MeshTopology.Quads, mesh.GetTopology(2), "Quads");
        Expect.Equal(MeshTopology.Points, mesh.GetTopology(3), "Points");
        Expect.Sequence(new[] { 0, 1, 1, 2 }, mesh.GetIndices(0), "line indices");
        Expect.Equal(14u, mesh.GetIndexStart(4), "the starts follow the earlier counts 4, 4, 4 and 2");
        Expect.Equal(3u, mesh.GetIndexCount(4), "the last count");
        Expect.Throws<ArgumentException>(() => mesh.SetIndices(new[] { 0, 1, 2 }, MeshTopology.Lines, 0), "three line indices");
        Expect.Throws<ArgumentException>(() => mesh.SetIndices(new[] { 0 }, MeshTopology.LineStrip, 0), "a line strip of one index");
        Expect.Throws<ArgumentException>(() => mesh.SetIndices(new[] { 0, 1, 2 }, MeshTopology.Quads, 0), "three quad indices");
        Expect.Throws<ArgumentException>(() => mesh.SetIndices(new[] { 0, 4 }, MeshTopology.Lines, 0), "a line index past the last vertex");
        Expect.Throws<ArgumentException>(() => mesh.SetIndices(new[] { 0, 1 }, (MeshTopology)99, 0), "an unknown topology");
        Expect.Throws<ArgumentOutOfRangeException>(() => mesh.SetIndices(new[] { 0, 1 }, MeshTopology.Lines, 5), "sub-mesh past the count");
        Expect.Equal(MeshTopology.Lines, mesh.GetTopology(0), "failed sets keep the topology");
        Expect.Throws<InvalidOperationException>(() => mesh.GetTriangles(0), "GetTriangles on Lines");
        Expect.Throws<InvalidOperationException>(() => mesh.GetTriangles(new List<int>(), 3), "GetTriangles(List) on Points");
        Expect.Throws<InvalidOperationException>(() => _ = mesh.triangles, "triangles with a Lines sub-mesh");
        mesh.SetTriangles(new[] { 0, 1, 2 }, 3, false);
        Expect.Equal(MeshTopology.Triangles, mesh.GetTopology(3), "SetTriangles makes the sub-mesh a triangle list");
    }

    private static void VertexCountChanges()
    {
        var mesh = QuadMesh();
        mesh.normals = new[] { Vector3.up, Vector3.up, Vector3.up, Vector3.up };
        mesh.uv = new Vector2[4];
        mesh.colors32 = new Color32[4];
        mesh.vertices = new[] { Vector3.one, Vector3.one, Vector3.one, Vector3.one };
        Expect.Equal(12, mesh.normals.Length + mesh.uv.Length + mesh.colors32.Length, "the same count keeps every channel");
        mesh.vertices = new Vector3[6];
        Expect.Equal(0, mesh.normals.Length + mesh.uv.Length + mesh.colors32.Length, "a different count drops the channels that no longer fit");
        Expect.Sequence(new[] { 0, 1, 2, 0, 2, 3 }, mesh.triangles, "indices still in range survive");
        mesh.normals = new Vector3[6];
        mesh.uv2 = new Vector2[0];
        mesh.vertices = new Vector3[6];
        Expect.Equal(6, mesh.normals.Length, "an unchanged count keeps a channel");
        var error = Expect.Catch<InvalidOperationException>(() => mesh.vertices = new Vector3[3], "shrinking below an index");
        Expect.That(error.Message.Contains("sub-mesh 0", StringComparison.Ordinal), $"the message names the sub-mesh: {error.Message}");
        Expect.Equal(6, mesh.vertexCount, "a refused change keeps the positions");
        Expect.Equal(6, mesh.normals.Length, "a refused change keeps the other channels");
        Expect.Equal(6u, mesh.GetIndexCount(0), "a refused change keeps the indices");
        mesh.subMeshCount = 2;
        mesh.SetTriangles(new List<int> { 0, 1, 2 }, 1, false, 3);
        Expect.Throws<InvalidOperationException>(() => mesh.vertices = new Vector3[5], "the base vertex counts toward the range");
        mesh.triangles = [];
        mesh.vertices = new Vector3[2];
        Expect.Equal(2, mesh.vertexCount, "without indices any count is fine");
    }

    private static void BoundsRules()
    {
        var mesh = new Mesh { vertices = [new(-1, 0, 2), new(3, 4, -2), new(0, 1, 0)] };
        Expect.Equal(default, mesh.bounds, "setting vertices leaves the bounds alone");
        mesh.RecalculateBounds();
        Expect.Equal(new Bounds(new Vector3(1, 2, 0), new Vector3(4, 4, 4)), mesh.bounds, "RecalculateBounds covers the positions");
        mesh.bounds = new Bounds(Vector3.one, Vector3.one * 8);
        Expect.Equal(new Bounds(Vector3.one, Vector3.one * 8), mesh.bounds, "bounds set explicitly");
        mesh.triangles = [0, 1, 2];
        Expect.Equal(new Bounds(new Vector3(1, 2, 0), new Vector3(4, 4, 4)), mesh.bounds, "the triangles setter recalculates the bounds");
        mesh.bounds = new Bounds(Vector3.one, Vector3.one);
        mesh.SetTriangles(new List<int> { 0, 1, 2 }, 0, false);
        Expect.Equal(new Bounds(Vector3.one, Vector3.one), mesh.bounds, "SetTriangles with calculateBounds false keeps the bounds");
        mesh.SetTriangles(new List<int> { 0, 1, 2 }, 0);
        Expect.Equal(new Bounds(new Vector3(1, 2, 0), new Vector3(4, 4, 4)), mesh.bounds, "SetTriangles calculates the bounds by default");
        mesh.bounds = new Bounds(Vector3.one, Vector3.one);
        mesh.SetIndices(new[] { 0, 1 }, MeshTopology.Lines, 0, false);
        Expect.Equal(new Bounds(Vector3.one, Vector3.one), mesh.bounds, "SetIndices with calculateBounds false keeps the bounds");
        mesh.SetIndices(new[] { 0, 1 }, MeshTopology.Lines, 0);
        Expect.Equal(new Bounds(new Vector3(1, 2, 0), new Vector3(4, 4, 4)), mesh.bounds, "SetIndices calculates the bounds by default");
        mesh.vertices = [new Vector3(5, 5, 5), new(6, 6, 6), new(7, 7, 7)];
        Expect.Equal(new Bounds(new Vector3(1, 2, 0), new Vector3(4, 4, 4)), mesh.bounds, "replacing the vertices does not move the bounds");
        var empty = new Mesh { bounds = new Bounds(Vector3.one, Vector3.one) };
        empty.RecalculateBounds();
        Expect.Equal(default, empty.bounds, "an empty mesh has zero bounds");
    }
}
