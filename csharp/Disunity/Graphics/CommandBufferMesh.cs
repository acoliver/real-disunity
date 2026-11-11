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
    /// it is when the buffer is executed. A property block that holds values is copied now and drawn as the draw's own block,
    /// between its per-draw values and the material, so later edits to the block change no recorded draw; an empty or null block
    /// records none. A block that holds an array or a buffer throws <see cref="NotSupportedException"/>.</summary>
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
        if (properties is not null) ThrowIfUnsent(properties.Properties);
        var block = properties is { isEmpty: false } ? CopyOf(properties.Properties) : null;
        Record(() =>
        {
            // Everything that can throw comes before the block is staged, so a refused draw leaves nothing for the next one.
            var (identity, world) = (mesh.RendererIdentity, material.WorldLink.Id);
            if (block is not null) WorldGraphics.StageDrawBlock(block);
            WorldGraphics.RecordDrawMesh(identity, matrix, world, submeshIndex, shaderPass);
        });
    }

    /// <summary>A draw's block reaches the host as floats, integers, vectors, matrices and textures; arrays and buffers have no
    /// way there, so a block that holds one throws rather than draw without it.</summary>
    private static void ThrowIfUnsent(PropertyStore block)
    {
        var kind = block.FloatArrays.Count > 0 || block.VectorArrays.Count > 0 || block.ColorArrays.Count > 0 || block.MatrixArrays.Count > 0
            ? "an array"
            : block.Buffers.Count > 0 || block.ConstantBuffers.Count > 0 ? "a buffer" : null;
        if (kind is not null)
            throw new NotSupportedException($"{DrawMeshMember}(properties): a property block that holds {kind} is not supported yet");
    }

    private static PropertyStore CopyOf(PropertyStore source)
    {
        var copy = new PropertyStore();
        copy.CopyFrom(source);
        return copy;
    }
}
