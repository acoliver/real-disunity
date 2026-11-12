namespace UnityEngine.Rendering;

public sealed partial class CommandBuffer
{
    private const string DrawMeshMember = "CommandBuffer.DrawMesh";

    public void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material) => RecordDrawMesh(mesh, matrix, material, 0, -1, null);

    public void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int submeshIndex) =>
        RecordDrawMesh(mesh, matrix, material, submeshIndex, -1, null);

    public void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int submeshIndex, int shaderPass) =>
        RecordDrawMesh(mesh, matrix, material, submeshIndex, shaderPass, null);

    /// <summary>Records a draw of the mesh into the targets bound at this point of the buffer, with the view and projection set
    /// then. A shader pass of -1 draws every pass. The host reads the mesh by its renderer identity, and the mesh is stored
    /// when the buffer is executed if the loaded world does not hold its current contents, so the draw shows the mesh as it is
    /// then. A mesh or material destroyed, or a sub-mesh the mesh lost, after recording is refused when the buffer runs. A property block that holds a value
    /// throws <see cref="NotSupportedException"/>; an empty or null block is accepted.</summary>
    public void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int submeshIndex, int shaderPass, MaterialPropertyBlock? properties) =>
        RecordDrawMesh(mesh, matrix, material, submeshIndex, shaderPass, properties);

    private void RecordDrawMesh(Mesh? mesh, Matrix4x4 matrix, Material? material, int submeshIndex, int shaderPass, MaterialPropertyBlock? properties)
    {
        ThrowIfReleased();
        ArgumentNullException.ThrowIfNull(mesh);
        ArgumentNullException.ThrowIfNull(material);
        ThrowIfDestroyed(mesh, material);
        ThrowIfSubMeshOutside(mesh, submeshIndex);
        if (shaderPass < -1) throw new ArgumentException($"{DrawMeshMember}: shader pass {shaderPass} is below -1", nameof(shaderPass));
        if (properties is { isEmpty: false })
            throw new NotSupportedException($"{DrawMeshMember}(properties): a property block that holds values is not supported yet");
        Record(() => DrawMeshNow(mesh, matrix, material, submeshIndex, shaderPass));
    }

    private static void ThrowIfDestroyed(Mesh mesh, Material material)
    {
        if (!mesh) throw new ArgumentException($"{DrawMeshMember}: the mesh was destroyed", nameof(mesh));
        if (!material) throw new ArgumentException($"{DrawMeshMember}: the material was destroyed", nameof(material));
    }

    private static void ThrowIfSubMeshOutside(Mesh mesh, int submeshIndex)
    {
        if (submeshIndex < 0 || submeshIndex >= mesh.subMeshCount)
            throw new ArgumentException($"{DrawMeshMember}: sub-mesh {submeshIndex} is outside the mesh's {mesh.subMeshCount} sub-meshes", nameof(submeshIndex));
    }

    /// <summary>The draw as the buffer runs it. Both objects and the sub-mesh are checked again before the identity or the
    /// world's material is asked for, because either would store or make a resource for a draw the world cannot show: the
    /// mesh or material may be destroyed, or the mesh may have lost the sub-mesh, since recording.</summary>
    private static void DrawMeshNow(Mesh mesh, Matrix4x4 matrix, Material material, int submeshIndex, int shaderPass)
    {
        ThrowIfDestroyed(mesh, material);
        ThrowIfSubMeshOutside(mesh, submeshIndex);
        WorldGraphics.RecordDrawMesh(mesh.RendererIdentity, matrix, material.WorldLink.Id, submeshIndex, shaderPass);
    }
}
