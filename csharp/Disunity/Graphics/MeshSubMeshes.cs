using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>Sub-meshes: indices with a topology and a base vertex. A sub-mesh's index start is the running sum of the
/// index counts before it. A new mesh has one empty triangle sub-mesh.</summary>
public partial class Mesh
{
    private const int MaxUInt16Index = 65535;

    private sealed class SubMesh
    {
        public int[] Indices = [];
        public MeshTopology Topology = MeshTopology.Triangles;
        public int BaseVertex;
    }

    private readonly List<SubMesh> _subMeshes = [];
    private IndexFormat _indexFormat = IndexFormat.UInt16;

    /// <summary>The engine's choice with <see cref="IndexFormat.UInt16"/>: an index above 65535 is refused with an
    /// <see cref="ArgumentException"/> instead of being truncated, and narrowing a mesh that holds one is refused.</summary>
    public IndexFormat indexFormat
    {
        get => _indexFormat;
        set
        {
            if (value == IndexFormat.UInt16 && _subMeshes.Exists(subMesh => Array.Exists(subMesh.Indices, index => index > MaxUInt16Index)))
            {
                throw new InvalidOperationException("indexFormat: a sub-mesh holds an index above 65535, which UInt16 cannot store");
            }
            _indexFormat = value;
        }
    }

    public int subMeshCount
    {
        get => _subMeshes.Count;
        set => ResizeSubMeshes(value);
    }

    /// <summary>All sub-meshes as one triangle list, with each base vertex applied. Setting it replaces every
    /// sub-mesh with one, and recalculates <see cref="bounds"/>. The getter throws when a sub-mesh is not a triangle list.</summary>
    public int[] triangles
    {
        get => ReadAllTriangles();
        set => ReplaceWithTriangles(value);
    }

    public int GetIndexStart(int submesh)
    {
        CheckSubMesh(submesh);
        var start = 0;
        for (var index = 0; index < submesh; index++) start += _subMeshes[index].Indices.Length;
        return start;
    }

    public int GetIndexCount(int submesh) => SubMeshAt(submesh).Indices.Length;

    public int GetBaseVertex(int submesh) => SubMeshAt(submesh).BaseVertex;

    public MeshTopology GetTopology(int submesh) => SubMeshAt(submesh).Topology;

    public int[] GetIndices(int submesh) => ReadIndices(submesh, true);
    public int[] GetIndices(int submesh, bool applyBaseVertex) => ReadIndices(submesh, applyBaseVertex);
    public void GetIndices(List<int> indices, int submesh) => Fill(indices, ReadIndices(submesh, true));
    public void GetIndices(List<int> indices, int submesh, bool applyBaseVertex) => Fill(indices, ReadIndices(submesh, applyBaseVertex));

    public int[] GetTriangles(int submesh) => ReadTriangles(submesh, true);
    public int[] GetTriangles(int submesh, bool applyBaseVertex) => ReadTriangles(submesh, applyBaseVertex);
    public void GetTriangles(List<int> triangles, int submesh) => Fill(triangles, ReadTriangles(submesh, true));
    public void GetTriangles(List<int> triangles, int submesh, bool applyBaseVertex) => Fill(triangles, ReadTriangles(submesh, applyBaseVertex));

    public void SetTriangles(int[] triangles, int submesh) => StoreTriangles(triangles, submesh, true, 0);
    public void SetTriangles(int[] triangles, int submesh, bool calculateBounds) => StoreTriangles(triangles, submesh, calculateBounds, 0);
    public void SetTriangles(int[] triangles, int submesh, bool calculateBounds, int baseVertex) => StoreTriangles(triangles, submesh, calculateBounds, baseVertex);
    public void SetTriangles(List<int> triangles, int submesh) => StoreTriangles(triangles, submesh, true, 0);
    public void SetTriangles(List<int> triangles, int submesh, bool calculateBounds) => StoreTriangles(triangles, submesh, calculateBounds, 0);
    public void SetTriangles(List<int> triangles, int submesh, bool calculateBounds, int baseVertex) => StoreTriangles(triangles, submesh, calculateBounds, baseVertex);

    public void SetIndices(int[] indices, MeshTopology topology, int submesh) => StoreSubMesh(submesh, Map(indices, index => index), topology, true, 0);
    public void SetIndices(int[] indices, MeshTopology topology, int submesh, bool calculateBounds) => StoreSubMesh(submesh, Map(indices, index => index), topology, calculateBounds, 0);
    public void SetIndices(int[] indices, MeshTopology topology, int submesh, bool calculateBounds, int baseVertex) => StoreSubMesh(submesh, Map(indices, index => index), topology, calculateBounds, baseVertex);
    public void SetIndices(List<int> indices, MeshTopology topology, int submesh) => StoreSubMesh(submesh, Map(indices, index => index), topology, true, 0);
    public void SetIndices(List<int> indices, MeshTopology topology, int submesh, bool calculateBounds) => StoreSubMesh(submesh, Map(indices, index => index), topology, calculateBounds, 0);
    public void SetIndices(List<int> indices, MeshTopology topology, int submesh, bool calculateBounds, int baseVertex) => StoreSubMesh(submesh, Map(indices, index => index), topology, calculateBounds, baseVertex);

    private void StoreTriangles(IReadOnlyList<int>? triangles, int submesh, bool calculateBounds, int baseVertex) =>
        StoreSubMesh(submesh, Map(triangles, index => index), MeshTopology.Triangles, calculateBounds, baseVertex);

    private void ResetSubMeshes()
    {
        _subMeshes.Clear();
        _subMeshes.Add(new SubMesh());
    }

    private SubMesh SubMeshAt(int submesh)
    {
        CheckSubMesh(submesh);
        return _subMeshes[submesh];
    }

    private void CheckSubMesh(int submesh)
    {
        if (submesh < 0 || submesh >= _subMeshes.Count)
        {
            throw new ArgumentOutOfRangeException(nameof(submesh), submesh, $"the mesh has {_subMeshes.Count} sub-meshes; set subMeshCount before using a new one");
        }
    }

    private void ResizeSubMeshes(int count)
    {
        RequireReadable();
        ArgumentOutOfRangeException.ThrowIfNegative(count);
        if (count < _subMeshes.Count) _subMeshes.RemoveRange(count, _subMeshes.Count - count);
        while (_subMeshes.Count < count) _subMeshes.Add(new SubMesh());
    }

    private int[] ReadIndices(int submesh, bool applyBaseVertex)
    {
        RequireReadable();
        var source = SubMeshAt(submesh);
        var offset = applyBaseVertex ? source.BaseVertex : 0;
        return Map(source.Indices, index => index + offset);
    }

    private int[] ReadTriangles(int submesh, bool applyBaseVertex)
    {
        if (SubMeshAt(submesh).Topology != MeshTopology.Triangles)
        {
            throw new InvalidOperationException($"sub-mesh {submesh} is {SubMeshAt(submesh).Topology}, not a triangle list");
        }
        return ReadIndices(submesh, applyBaseVertex);
    }

    private int[] ReadAllTriangles()
    {
        var all = new List<int>();
        for (var submesh = 0; submesh < _subMeshes.Count; submesh++) all.AddRange(ReadTriangles(submesh, true));
        return all.ToArray();
    }

    private void ReplaceWithTriangles(IReadOnlyList<int>? triangles)
    {
        RequireReadable();
        var indices = Map(triangles, index => index);
        CheckIndices(0, indices, MeshTopology.Triangles, 0);
        _subMeshes.Clear();
        _subMeshes.Add(new SubMesh { Indices = indices });
        RecalculateBounds();
    }

    private void StoreSubMesh(int submesh, int[] indices, MeshTopology topology, bool calculateBounds, int baseVertex)
    {
        RequireReadable();
        CheckSubMesh(submesh);
        CheckIndices(submesh, indices, topology, baseVertex);
        var target = _subMeshes[submesh];
        target.Indices = indices;
        target.Topology = topology;
        target.BaseVertex = baseVertex;
        if (calculateBounds) RecalculateBounds();
    }

    private void CheckIndices(int submesh, int[] indices, MeshTopology topology, int baseVertex)
    {
        if (!CountFitsTopology(topology, indices.Length))
        {
            throw new ArgumentException($"sub-mesh {submesh}: {indices.Length} indices do not form whole {topology} primitives");
        }
        var position = FirstOutOfRange(indices, baseVertex, vertexCount);
        if (position >= 0)
        {
            throw new ArgumentException($"sub-mesh {submesh}: index {indices[position]} with base vertex {baseVertex} is outside the {vertexCount} vertices");
        }
        if (_indexFormat == IndexFormat.UInt16 && Array.Exists(indices, index => index > MaxUInt16Index))
        {
            throw new ArgumentException($"sub-mesh {submesh}: an index above 65535 needs indexFormat UInt32");
        }
    }

    /// <summary>Whether a new vertex count still holds every index; the check runs before anything changes.</summary>
    private void CheckIndicesFit(int newCount)
    {
        for (var submesh = 0; submesh < _subMeshes.Count; submesh++)
        {
            var source = _subMeshes[submesh];
            var position = FirstOutOfRange(source.Indices, source.BaseVertex, newCount);
            if (position < 0) continue;
            throw new InvalidOperationException($"sub-mesh {submesh} indexes vertex {source.Indices[position] + source.BaseVertex}, outside the {newCount} vertices");
        }
    }

    private static int FirstOutOfRange(int[] indices, int baseVertex, int count)
    {
        for (var position = 0; position < indices.Length; position++)
        {
            var vertex = (long)indices[position] + baseVertex;
            if (indices[position] < 0 || vertex < 0 || vertex >= count) return position;
        }
        return -1;
    }

    private static bool CountFitsTopology(MeshTopology topology, int count) => topology switch
    {
        MeshTopology.Triangles => count % 3 == 0,
        MeshTopology.Quads => count % 4 == 0,
        MeshTopology.Lines => count % 2 == 0,
        MeshTopology.LineStrip => count == 0 || count >= 2,
        MeshTopology.Points => true,
        _ => throw new ArgumentException($"topology {(int)topology} is not a mesh topology"),
    };
}
