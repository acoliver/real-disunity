namespace UnityEngine.Rendering;

/// <summary>
/// The procedural draws of a command buffer: a material's pass drawn over vertices its shader generates from the vertex and
/// instance index, with no mesh. The forms without an index buffer record a draw into the targets bound at this point of the
/// buffer, with the view and projection set then. They take triangle and line topologies; the others, a property block that
/// holds a value, and the forms with an index buffer throw <see cref="NotSupportedException"/> naming the member and record
/// nothing. A null or destroyed material, a pass below -1, a matrix component that is not finite and a negative count throw
/// when called and record nothing. A released buffer throws <see cref="ObjectDisposedException"/> first.
/// </summary>
public sealed partial class CommandBuffer
{
    private const string DrawProceduralMember = "CommandBuffer.DrawProcedural";

    public void DrawProcedural(Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int vertexCount, int instanceCount,
        MaterialPropertyBlock? properties) => RecordDrawProcedural(matrix, material, shaderPass, topology, (vertexCount, instanceCount), properties);

    public void DrawProcedural(Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int vertexCount, int instanceCount) =>
        RecordDrawProcedural(matrix, material, shaderPass, topology, (vertexCount, instanceCount), null);

    public void DrawProcedural(Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int vertexCount) =>
        RecordDrawProcedural(matrix, material, shaderPass, topology, (vertexCount, 1), null);

    public void DrawProcedural(GraphicsBuffer indexBuffer, Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int indexCount,
        int instanceCount, MaterialPropertyBlock properties) => Fail(nameof(DrawProcedural) + " with an index buffer");

    public void DrawProcedural(GraphicsBuffer indexBuffer, Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int indexCount,
        int instanceCount) => Fail(nameof(DrawProcedural) + " with an index buffer");

    public void DrawProcedural(GraphicsBuffer indexBuffer, Matrix4x4 matrix, Material material, int shaderPass, MeshTopology topology, int indexCount) =>
        Fail(nameof(DrawProcedural) + " with an index buffer");

    private void RecordDrawProcedural(Matrix4x4 matrix, Material? material, int shaderPass, MeshTopology topology, (int Vertices, int Instances) counts,
        MaterialPropertyBlock? properties)
    {
        ThrowIfReleased();
        ArgumentNullException.ThrowIfNull(material);
        if (!material) throw new ArgumentException($"{DrawProceduralMember}: the material was destroyed", nameof(material));
        if (shaderPass < -1) throw new ArgumentException($"{DrawProceduralMember}: shader pass {shaderPass} is below -1", nameof(shaderPass));
        for (var row = 0; row < 4; row++)
            for (var column = 0; column < 4; column++)
                if (!float.IsFinite(matrix[row, column]))
                    throw new ArgumentException($"{DrawProceduralMember}: matrix component ({row}, {column}) is not finite", nameof(matrix));
        ArgumentOutOfRangeException.ThrowIfNegative(counts.Vertices, "vertexCount");
        ArgumentOutOfRangeException.ThrowIfNegative(counts.Instances, "instanceCount");
        var wireTopology = topology switch
        {
            MeshTopology.Triangles => 0u,
            MeshTopology.Lines => 1u,
            _ => throw new NotSupportedException($"{DrawProceduralMember}: the {topology} topology is not supported yet; triangles and lines are"),
        };
        if (properties is { isEmpty: false })
            throw new NotSupportedException($"{DrawProceduralMember}(properties): a property block that holds values is not supported yet");
        Record(() => DrawProceduralNow(matrix, material, shaderPass, wireTopology, counts));
    }

    /// <summary>The draw as the buffer runs it. The material is checked again, since it may have been destroyed after
    /// recording, before the world's material is asked for.</summary>
    private static void DrawProceduralNow(Matrix4x4 matrix, Material material, int shaderPass, uint topology, (int Vertices, int Instances) counts)
    {
        if (!material) throw new ArgumentException($"{DrawProceduralMember}: the material was destroyed", nameof(material));
        WorldGraphics.RecordDrawProcedural(matrix, material.WorldLink.Id, shaderPass, topology, counts);
    }
}
