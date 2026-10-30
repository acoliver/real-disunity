using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>
/// A <see cref="Renderer"/> that draws the mesh of the <see cref="MeshFilter"/> on its object. Its bounds are the
/// world-space box of that mesh's bounds, and the zero box without a filter or a mesh. The native component holds
/// the settings; <see cref="additionalVertexStreams"/> is stored and nothing reads it yet. A renderer and a
/// <see cref="SkinnedMeshRenderer"/> cannot share an object. Nothing draws the renderer yet.
/// </summary>
public sealed class MeshRenderer : Renderer
{
    private unsafe DisunityMeshRenderer Read()
    {
        DisunityMeshRenderer value;
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_read(DisunityHost.WorldHandle, Handle, &value));
        return value;
    }

    internal override DisunityMeshRenderer ReadSettings() => Read();

    internal override unsafe void WriteSettings(DisunityMeshRenderer value) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_mesh_renderer_write(DisunityHost.WorldHandle, Handle, &value));

    internal override Mesh? BoundsMesh => GetComponent<MeshFilter>()?.sharedMesh;

    /// <summary>Per-vertex data that replaces the mesh's own on this renderer. It is stored and returned as given.</summary>
    public Mesh? additionalVertexStreams { get; set; }

    /// <summary>Always 0: the engine draws every sub-mesh from the first.</summary>
    public int subMeshStartIndex => 0;

    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        base.CopyManagedState(source, remap);
        additionalVertexStreams = ((MeshRenderer)source).additionalVertexStreams;
    }
}
