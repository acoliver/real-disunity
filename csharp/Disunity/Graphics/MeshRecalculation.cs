namespace UnityEngine;

public partial class Mesh
{
    /// <summary>A copy of the data for later render work; it works on an unreadable mesh.</summary>
    internal MeshData Snapshot() => new()
    {
        Positions = (Vector3[])_positions.Clone(),
        Normals = (Vector3[])_normals.Clone(),
        Tangents = (Vector4[])_tangents.Clone(),
        Colors = (Color[])_colors.Clone(),
        Uvs = Map(_uvs, channel => (Vector4[])channel.Clone()),
        UvDimensions = (int[])_uvDimensions.Clone(),
        SubMeshes = Map(_subMeshes, subMesh => new MeshSubMeshData((int[])subMesh.Indices.Clone(), subMesh.Topology, subMesh.BaseVertex)),
        Bounds = _bounds,
        IndexFormat = _indexFormat,
    };

    /// <summary>Sets <see cref="bounds"/> to the box around every position; an empty mesh gets zero bounds.</summary>
    public void RecalculateBounds()
    {
        RequireReadable();
        if (_positions.Length == 0)
        {
            _bounds = default;
            return;
        }
        var min = _positions[0];
        var max = _positions[0];
        foreach (var position in _positions)
        {
            min = Vector3.Min(min, position);
            max = Vector3.Max(max, position);
        }
        var box = new Bounds();
        box.SetMinMax(min, max);
        _bounds = box;
    }

    /// <summary>
    /// Replaces the normals with the area-weighted average of the faces around each vertex. Vertices are
    /// smoothed by index: two vertices at the same position do not share faces. Every sub-mesh that is a triangle
    /// list counts, with its base vertex applied, and the others are ignored. A vertex without a triangle gets
    /// (0, 0, 0). A triangle faces the side its vertices run clockwise from, so a quad in the XY plane wound
    /// clockwise as seen from -z has normal (0, 0, -1).
    /// </summary>
    public void RecalculateNormals()
    {
        RequireReadable();
        var sums = new Vector3[vertexCount];
        foreach (var subMesh in _subMeshes)
        {
            if (subMesh.Topology != MeshTopology.Triangles) continue;
            for (var at = 0; at + 2 < subMesh.Indices.Length; at += 3)
            {
                var corners = Corners(subMesh, at);
                var weighted = Vector3.Cross(_positions[corners.Item2] - _positions[corners.Item1], _positions[corners.Item3] - _positions[corners.Item1]);
                sums[corners.Item1] += weighted;
                sums[corners.Item2] += weighted;
                sums[corners.Item3] += weighted;
            }
        }
        _normals = Map(sums, UnitOrZero);
    }

    /// <summary>
    /// Replaces the tangents using <see cref="normals"/> and the first uv channel. Each triangle gives the direction
    /// in which u grows; the vertex sums them, removes the part along its normal and normalizes. w is +1 when the
    /// normal, tangent and the direction in which v grows form the engine's left-handed frame, otherwise -1.
    /// A vertex whose triangles have no usable uv gets an arbitrary unit vector perpendicular to its normal and w = 1.
    /// </summary>
    public void RecalculateTangents()
    {
        RequireReadable();
        if (_normals.Length != vertexCount || vertexCount == 0) throw new InvalidOperationException("RecalculateTangents needs normals; set them or call RecalculateNormals first");
        if (_uvs[0].Length != vertexCount) throw new InvalidOperationException("RecalculateTangents needs the uv channel 0 (uv)");
        var tangentSums = new Vector3[vertexCount];
        var bitangentSums = new Vector3[vertexCount];
        foreach (var subMesh in _subMeshes)
        {
            if (subMesh.Topology != MeshTopology.Triangles) continue;
            for (var at = 0; at + 2 < subMesh.Indices.Length; at += 3) AddTriangleTangent(Corners(subMesh, at), tangentSums, bitangentSums);
        }
        var result = new Vector4[vertexCount];
        for (var vertex = 0; vertex < result.Length; vertex++) result[vertex] = FinishTangent(_normals[vertex], tangentSums[vertex], bitangentSums[vertex]);
        _tangents = result;
    }

    private static (int, int, int) Corners(SubMesh subMesh, int at) =>
        (subMesh.Indices[at] + subMesh.BaseVertex, subMesh.Indices[at + 1] + subMesh.BaseVertex, subMesh.Indices[at + 2] + subMesh.BaseVertex);

    private void AddTriangleTangent((int, int, int) corners, Vector3[] tangentSums, Vector3[] bitangentSums)
    {
        var (first, second, third) = corners;
        var edge1 = _positions[second] - _positions[first];
        var edge2 = _positions[third] - _positions[first];
        var du1 = _uvs[0][second].x - _uvs[0][first].x;
        var dv1 = _uvs[0][second].y - _uvs[0][first].y;
        var du2 = _uvs[0][third].x - _uvs[0][first].x;
        var dv2 = _uvs[0][third].y - _uvs[0][first].y;
        var determinant = du1 * dv2 - du2 * dv1;
        if (determinant == 0f || !float.IsFinite(determinant)) return;
        var tangent = (edge1 * dv2 - edge2 * dv1) / determinant;
        var bitangent = (edge2 * du1 - edge1 * du2) / determinant;
        foreach (var vertex in new[] { first, second, third })
        {
            tangentSums[vertex] += tangent;
            bitangentSums[vertex] += bitangent;
        }
    }

    private static Vector4 FinishTangent(Vector3 normal, Vector3 tangentSum, Vector3 bitangentSum)
    {
        var tangent = UnitOrZero(tangentSum - normal * Vector3.Dot(normal, tangentSum));
        if (tangent == Vector3.zero)
        {
            var unitNormal = UnitOrZero(normal);
            var fallback = Vector3.zero;
            Vector3.OrthoNormalize(ref unitNormal, ref fallback);
            return new Vector4(fallback.x, fallback.y, fallback.z, 1f);
        }
        var handedness = Vector3.Dot(Vector3.Cross(normal, tangent), bitangentSum) < 0f ? -1f : 1f;
        return new Vector4(tangent.x, tangent.y, tangent.z, handedness);
    }

    private static Vector3 UnitOrZero(Vector3 value)
    {
        var length = MathF.Sqrt(Vector3.Dot(value, value));
        return length > 0f && float.IsFinite(length) ? value / length : Vector3.zero;
    }
}
