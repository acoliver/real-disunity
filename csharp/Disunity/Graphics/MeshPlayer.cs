using Disunity.Hosting;
using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>What the host is given of a mesh: its contents under a renderer identity, stored when a mesh filter takes the
/// mesh or a command buffer draws it, and again after the data changes: before the next frame is drawn, or sooner when
/// something asks for the identity. A world that was unloaded took the stored contents with it, so the loaded world is
/// given them again.</summary>
public partial class Mesh
{
    private const uint ChannelNormals = 1, ChannelUv0 = 2, ChannelColors = 4;

    private ulong _identity;

    /// <summary>The loaded world that holds this mesh's contents under <see cref="_identity"/>, which is set when both
    /// native stores of the contents have succeeded and holds nothing for a world that was unloaded.</summary>
    private WorldGraphics.Link _stored;

    /// <summary>The identity the host knows this mesh's contents by, from <see cref="DisunityHost.FirstPlayerMeshIdentity"/>
    /// up: allocated the first time it is asked for and kept for the mesh's lifetime. The loaded world holds the mesh's
    /// current contents under it when this returns: a mesh the world does not hold, or one changed since it was stored, is
    /// stored first, so a draw that runs now shows the mesh as it is now and the next frame flush has nothing left to send
    /// for it. A store that fails throws and is not queued again: the mesh no longer counts as stored, so the next call
    /// stores it again and a frame flush does not.</summary>
    internal ulong RendererIdentity
    {
        get
        {
            if (_identity == 0) _identity = DisunityHost.AllocatePlayerMeshIdentity();
            // The mesh leaves the changed set before its data is read, so an edit made during the store queues it again.
            var changed = DisunityHost.TakeQueuedMeshStore(this);
            if (!changed && _stored.IsCurrent) return _identity;
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
    /// another topology are counted as left out and not sent. Ignores <see cref="isReadable"/>. The two native stores are
    /// not one transaction: a store the host refuses part way can leave the fixed mesh stored and its channels dropped.
    /// The mesh therefore stops counting as stored before anything is read or sent, and counts again only when both
    /// stores have succeeded.</summary>
    internal unsafe void StoreForRenderer()
    {
        _stored = default;
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
        _stored = new WorldGraphics.Link(DisunityHost.WorldHandle, _identity);
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
