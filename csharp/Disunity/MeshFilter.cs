using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// Holds the mesh a <see cref="MeshRenderer"/> on the same object draws. The native component stores the mesh's
/// instance id, so a clone from <c>Instantiate</c> names the same mesh. The component also keeps the mesh alive.
/// <see cref="mesh"/> gives the filter its own copy of the shared mesh on first use, named "&lt;name&gt; Instance",
/// assigns it as the shared mesh and returns the same copy afterwards. Setting <see cref="mesh"/> assigns the shared
/// mesh (the engine's choice), so the next read makes a copy of it. Nothing draws the mesh yet.
/// </summary>
public sealed class MeshFilter : NativeComponent
{
    private Mesh? _mesh;
    private Mesh? _instance;

    private unsafe ulong ReadId()
    {
        ulong id;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_filter_read(DisunityHost.WorldHandle, Handle, &id));
        return id;
    }

    /// <summary>The mesh the filter names, or null. A destroyed mesh is returned as it is.</summary>
    public Mesh? sharedMesh
    {
        get
        {
            var id = ReadId();
            if (id == 0) return null;
            return _mesh is not null && unchecked((ulong)(uint)_mesh.GetInstanceID()) == id ? _mesh : CanvasRenderer.Find<Mesh>(id);
        }
        set
        {
            var id = CanvasRenderer.IdOf(value, nameof(value));
            DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_filter_write(DisunityHost.WorldHandle, Handle, id));
            _mesh = value;
        }
    }

    /// <summary>This filter's own copy of the shared mesh, made on first use; null when there is no shared mesh.</summary>
    public Mesh? mesh
    {
        get
        {
            var shared = sharedMesh;
            if (shared is null || ReferenceEquals(shared, _instance)) return shared;
            var copy = Object.Instantiate(shared);
            copy.name = shared.name + " Instance";
            _instance = copy;
            sharedMesh = copy;
            return copy;
        }
        set => sharedMesh = value;
    }

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap) =>
        _mesh = ((MeshFilter)source).sharedMesh;
}
