using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>What the host is given of a mesh: its contents under a renderer identity, stored when a mesh filter takes the
/// mesh and again, before the next frame is drawn, after the data changes.</summary>
public partial class Mesh
{
    private const uint ChannelNormals = 1, ChannelUv0 = 2, ChannelColors = 4;

    private ulong _identity;

    /// <summary>The identity the host knows this mesh's contents by, from <see cref="DisunityHost.FirstPlayerMeshIdentity"/>
    /// up: allocated the first time a mesh filter takes the mesh (which stores its data), and kept for its lifetime.</summary>
    internal ulong RendererIdentity
    {
        get
        {
            if (_identity != 0) return _identity;
            _identity = DisunityHost.AllocatePlayerMeshIdentity();
            StoreForRenderer();
            return _identity;
        }
    }

    /// <summary>Queues a store of the mesh's data once it has an identity.</summary>
    private void Changed()
    {
        if (_identity != 0) DisunityHost.QueueMeshStore(this);
    }

    /// <summary>Hands the mesh's contents to the host: one vertex per position with the normal, first uv and colour when
    /// the mesh has them, and the triangle indices of every triangle sub-mesh with its base vertex applied. Sub-meshes of
    /// another topology are counted as left out and not sent. Ignores <see cref="isReadable"/>.</summary>
    internal unsafe void StoreForRenderer()
    {
        DisunityHost.EnsureLoaded();
        var data = Snapshot();
        var vertices = Vertices(data, out var channels);
        var indices = new List<uint>();
        var runs = new List<DisunityMeshSubMesh>();
        var leftOut = TriangleRuns(data, indices, runs);
        var info = new DisunityMeshInfo
        {
            identity = _identity, channels = channels, vertex_count = (uint)vertices.Length,
            index_count = (uint)indices.Count, submesh_count = (uint)runs.Count, left_out = leftOut,
        };
        var indexArray = indices.ToArray();
        var runArray = runs.ToArray();
        fixed (DisunityMeshVertex* vertexPointer = vertices)
        fixed (uint* indexPointer = indexArray)
        fixed (DisunityMeshSubMesh* runPointer = runArray)
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_store(DisunityHost.WorldHandle, _identity, &info, vertexPointer, indexPointer, runPointer));
        StoreChannelsForRenderer(data);
    }

    private static DisunityMeshVertex[] Vertices(MeshData data, out uint channels)
    {
        var count = data.Positions.Length;
        var hasNormals = count > 0 && data.Normals.Length == count;
        var hasUv = count > 0 && data.Uvs[0].Length == count;
        var hasColors = count > 0 && data.Colors.Length == count;
        channels = (hasNormals ? ChannelNormals : 0) | (hasUv ? ChannelUv0 : 0) | (hasColors ? ChannelColors : 0);
        var vertices = new DisunityMeshVertex[count];
        for (var i = 0; i < count; i++)
        {
            var position = data.Positions[i];
            var normal = hasNormals ? data.Normals[i] : default;
            var uv = hasUv ? data.Uvs[0][i] : default;
            var color = hasColors ? data.Colors[i] : default;
            vertices[i] = new DisunityMeshVertex
            {
                x = position.x, y = position.y, z = position.z, nx = normal.x, ny = normal.y, nz = normal.z,
                u = uv.x, v = uv.y, r = color.r, g = color.g, b = color.b, a = color.a,
            };
        }
        return vertices;
    }

    /// <summary>Appends the indices of each non-empty triangle sub-mesh and its run, and returns how many non-empty
    /// sub-meshes of another topology were left out.</summary>
    private static uint TriangleRuns(MeshData data, List<uint> indices, List<DisunityMeshSubMesh> runs)
    {
        var leftOut = 0u;
        for (var slot = 0; slot < data.SubMeshes.Length; slot++)
        {
            var (subIndices, topology, baseVertex) = data.SubMeshes[slot];
            if (subIndices.Length == 0) continue;
            if (topology != MeshTopology.Triangles)
            {
                leftOut++;
                continue;
            }
            runs.Add(new DisunityMeshSubMesh { slot = (uint)slot, first_index = (uint)indices.Count, index_count = (uint)subIndices.Length });
            foreach (var index in subIndices) indices.Add(unchecked((uint)(index + baseVertex)));
        }
        return leftOut;
    }
}
