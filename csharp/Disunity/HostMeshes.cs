using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>The renderer identity of the first player mesh, above the player textures (which count up from
    /// <see cref="FirstPlayerTextureIdentity"/>), so the two ranges meet only after 2^40 textures; this is the engine's
    /// choice. A mesh takes its identity when it is assigned to a <see cref="MeshFilter"/> and keeps it.</summary>
    internal const ulong FirstPlayerMeshIdentity = 1UL << 41;

    private static ulong _nextPlayerMesh = FirstPlayerMeshIdentity;

    // Meshes with an identity whose data changed since the host last stored it. A script builds a mesh in many
    // calls (vertices, uvs, triangles), so the changes are stored once, just before the frame is drawn.
    private static readonly HashSet<Mesh> ChangedMeshes = [];

    internal static ulong AllocatePlayerMeshIdentity()
    {
        lock (ChangedMeshes) return _nextPlayerMesh++;
    }

    /// <summary>Notes that a mesh with an identity changed, so the next <see cref="StoreChangedMeshes"/> stores it.</summary>
    internal static void QueueMeshStore(Mesh mesh)
    {
        lock (ChangedMeshes) ChangedMeshes.Add(mesh);
    }

    /// <summary>Takes the mesh out of the changed set. True when it was there: the caller stores it now, and the next
    /// <see cref="StoreChangedMeshes"/> does not send it again.</summary>
    internal static bool TakeQueuedMeshStore(Mesh mesh)
    {
        lock (ChangedMeshes) return ChangedMeshes.Remove(mesh);
    }

    /// <summary>Stores every mesh that changed since the last call, except one destroyed since it changed. The scheduler runs
    /// it each frame, after LateUpdate. A store the host refuses throws; the mesh that failed is not queued again (it no
    /// longer counts as stored, so the next draw of it stores it again), and the meshes not yet reached stay queued for the
    /// next call. Edits made while the batch ran stay queued as well.</summary>
    internal static void StoreChangedMeshes()
    {
        Mesh[] batch;
        lock (ChangedMeshes)
        {
            batch = [.. ChangedMeshes];
            ChangedMeshes.Clear();
        }
        for (var done = 0; done < batch.Length; done++)
        {
            if (!batch[done]) continue;
            try { batch[done].StoreForRenderer(); }
            catch
            {
                QueueUnprocessed(batch, done + 1);
                throw;
            }
        }
    }

    private static void QueueUnprocessed(Mesh[] batch, int from)
    {
        lock (ChangedMeshes) ChangedMeshes.UnionWith(batch.Skip(from));
    }

    /// <summary>Forgets the changes of a world that is going; its meshes are not stored anywhere.</summary>
    private static void ClearChangedMeshes()
    {
        lock (ChangedMeshes) ChangedMeshes.Clear();
    }
}
