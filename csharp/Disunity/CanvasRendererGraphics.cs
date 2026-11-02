using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

/// <summary>What a <see cref="CanvasRenderer"/> hands to the native world to be drawn: the first sub-mesh of its mesh
/// and the renderer identity of its texture.</summary>
public sealed partial class CanvasRenderer
{
    /// <summary>Sends the mesh's vertices (position, first texture coordinate and color; a mesh without colors is white
    /// and one without texture coordinates has zeros) and the triangle indices of sub-mesh 0, with its base vertex
    /// applied, to the native renderer. A null mesh, or one whose first sub-mesh is not a triangle list, sends no
    /// indices. The native side rejects indices past the vertices.</summary>
    private unsafe void SendGeometry(Mesh? mesh)
    {
        var data = mesh?.Snapshot();
        var count = data?.Positions.Length ?? 0;
        var vertices = new DisunityUiVertex[count];
        for (var i = 0; i < count; i++)
        {
            var position = data!.Positions[i];
            var color = data.Colors.Length == count ? data.Colors[i] : Color.white;
            var uv = data.Uvs[0].Length == count ? data.Uvs[0][i] : default;
            vertices[i] = new DisunityUiVertex { x = position.x, y = position.y, z = position.z, u = uv.x, v = uv.y, r = color.r, g = color.g, b = color.b, a = color.a };
        }
        var indices = count == 0 ? [] : TriangleIndices(data!);
        fixed (DisunityUiVertex* vertexPointer = vertices)
        fixed (uint* indexPointer = indices)
            DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_geometry_write(
                DisunityHost.WorldHandle, Handle, count == 0 ? null : vertexPointer, (uint)count, indices.Length == 0 ? null : indexPointer, (uint)indices.Length));
    }

    private static uint[] TriangleIndices(MeshData data)
    {
        if (data.SubMeshes.Length == 0 || data.SubMeshes[0].Topology != MeshTopology.Triangles) return [];
        var (indices, baseVertex, _) = data.SubMeshes[0];
        var result = new uint[indices.Length];
        for (var i = 0; i < indices.Length; i++) result[i] = unchecked((uint)(indices[i] + baseVertex));
        return result;
    }

    /// <summary>Tells the native renderer which texture the quads sample. A <see cref="Texture2D"/> that has pixels
    /// gives its renderer identity, which stores its bytes for the host; no texture draws plain color; any other
    /// texture gives an identity with no texels, so the frame leaves its quads out.</summary>
    private void SendTexture(Texture? texture) =>
        DisunityHost.ThrowIfError(NativeBindings.disunity_canvas_renderer_texture_write(
            DisunityHost.WorldHandle, Handle, texture switch
            {
                null => 0UL,
                Texture2D { width: > 0, height: > 0 } plain => plain.RendererIdentity,
                _ => DisunityHost.UndrawnTextureIdentity(texture),
            }));
}
