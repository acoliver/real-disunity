namespace UnityEngine.Rendering;

/// <summary>
/// The procedural draws of a command buffer. Each form a compiled render pipeline calls is declared, because a game binds to the
/// overload it was built against, and throws <see cref="NotSupportedException"/> naming the member: the world's offscreen list
/// has no record that draws a material over generated vertices, so there is nothing to record yet.
/// </summary>
public sealed partial class CommandBuffer
{
    public void DrawProcedural(Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int vertexCount, int instanceCount,
        MaterialPropertyBlock properties) => Fail(nameof(DrawProcedural));

    public void DrawProcedural(Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int vertexCount, int instanceCount) =>
        Fail(nameof(DrawProcedural));

    public void DrawProcedural(Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int vertexCount) =>
        Fail(nameof(DrawProcedural));

    public void DrawProcedural(GraphicsBuffer indexBuffer, Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int indexCount,
        int instanceCount, MaterialPropertyBlock properties) => Fail(nameof(DrawProcedural));

    public void DrawProcedural(GraphicsBuffer indexBuffer, Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int indexCount,
        int instanceCount) => Fail(nameof(DrawProcedural));

    public void DrawProcedural(GraphicsBuffer indexBuffer, Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int indexCount) =>
        Fail(nameof(DrawProcedural));
}
