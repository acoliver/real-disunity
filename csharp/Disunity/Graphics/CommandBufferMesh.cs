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
    /// then. A mesh or material destroyed, or a sub-mesh the mesh lost, after recording is refused when the buffer runs. A
    /// property block that holds values is copied now and drawn as the draw's own block, between its per-draw values and the
    /// material, so later edits to the block change no recorded draw; an empty or null block records none. A block that holds
    /// an array or a buffer throws <see cref="NotSupportedException"/>.</summary>
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
        if (properties is not null) ThrowIfUnsent(properties.Properties);
        var block = properties is { isEmpty: false } ? CopyOf(properties.Properties) : null;
        Record(() => DrawMeshNow(mesh, matrix, material, submeshIndex, shaderPass, block));
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
    /// mesh or material may be destroyed, or the mesh may have lost the sub-mesh, since recording. Everything that can throw
    /// comes before the block is staged, so a refused draw leaves nothing for the next one.</summary>
    private static void DrawMeshNow(Mesh mesh, Matrix4x4 matrix, Material material, int submeshIndex, int shaderPass, PropertyStore? block)
    {
        ThrowIfDestroyed(mesh, material);
        ThrowIfSubMeshOutside(mesh, submeshIndex);
        var (identity, world) = (mesh.RendererIdentity, material.WorldLink.Id);
        if (block is not null) WorldGraphics.StageDrawBlock(block);
        WorldGraphics.RecordDrawMesh(identity, matrix, world, submeshIndex, shaderPass);
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
