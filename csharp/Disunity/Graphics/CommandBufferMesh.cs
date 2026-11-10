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
    /// then. A shader pass of -1 draws every pass. The host reads the mesh by its renderer identity, so the draw shows the mesh as
    /// it is when the buffer is executed. A property block that holds a value throws <see cref="NotSupportedException"/>; an empty
    /// or null block is accepted.</summary>
    public void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int submeshIndex, int shaderPass, MaterialPropertyBlock? properties) =>
        RecordDrawMesh(mesh, matrix, material, submeshIndex, shaderPass, properties);

    private void RecordDrawMesh(Mesh? mesh, Matrix4x4 matrix, Material? material, int submeshIndex, int shaderPass, MaterialPropertyBlock? properties)
    {
        ThrowIfReleased();
        ArgumentNullException.ThrowIfNull(mesh);
        ArgumentNullException.ThrowIfNull(material);
        if (!mesh) throw new ArgumentException($"{DrawMeshMember}: the mesh was destroyed", nameof(mesh));
        if (!material) throw new ArgumentException($"{DrawMeshMember}: the material was destroyed", nameof(material));
        if (submeshIndex < 0 || submeshIndex >= mesh.subMeshCount)
            throw new ArgumentException($"{DrawMeshMember}: sub-mesh {submeshIndex} is outside the mesh's {mesh.subMeshCount} sub-meshes", nameof(submeshIndex));
        if (shaderPass < -1) throw new ArgumentException($"{DrawMeshMember}: shader pass {shaderPass} is below -1", nameof(shaderPass));
        if (properties is { isEmpty: false })
            throw new NotSupportedException($"{DrawMeshMember}(properties): a property block that holds values is not supported yet");
        Record(() => WorldGraphics.RecordDrawMesh(mesh.RendererIdentity, matrix, material.WorldLink.Id, submeshIndex, shaderPass));
    }
}
