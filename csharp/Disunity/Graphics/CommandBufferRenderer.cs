namespace UnityEngine.Rendering;

public sealed partial class CommandBuffer
{
    private const string DrawRendererMember = "CommandBuffer.DrawRenderer";

    public void DrawRenderer(Renderer renderer, Material material) => RecordDrawRenderer(renderer, material, 0, -1);

    public void DrawRenderer(Renderer renderer, Material material, int submeshIndex) =>
        RecordDrawRenderer(renderer, material, submeshIndex, -1);

    /// <summary>Records a draw of the renderer's mesh with the given material, into the targets bound at this point of the buffer.
    /// The renderer is read when the buffer runs: its mesh filter's mesh, its transform and the property block set on it then,
    /// so a renderer moved between recording and execution draws where it is. A shader pass of -1 draws every pass. Only a
    /// <see cref="MeshRenderer"/> can be drawn; any other renderer throws <see cref="NotSupportedException"/>. A renderer or
    /// material destroyed after recording, a renderer with no mesh then, or a sub-mesh the mesh does not have then, is refused
    /// when the buffer runs.</summary>
    public void DrawRenderer(Renderer renderer, Material material, int submeshIndex, int shaderPass) =>
        RecordDrawRenderer(renderer, material, submeshIndex, shaderPass);

    private void RecordDrawRenderer(Renderer? renderer, Material? material, int submeshIndex, int shaderPass)
    {
        ThrowIfReleased();
        ArgumentNullException.ThrowIfNull(renderer);
        ArgumentNullException.ThrowIfNull(material);
        if (renderer is not MeshRenderer)
            throw new NotSupportedException($"{DrawRendererMember}: a {renderer.GetType().Name} is not supported yet");
        if (!renderer) throw new ArgumentException($"{DrawRendererMember}: the renderer was destroyed", nameof(renderer));
        if (!material) throw new ArgumentException($"{DrawRendererMember}: the material was destroyed", nameof(material));
        if (submeshIndex < 0) throw new ArgumentException($"{DrawRendererMember}: sub-mesh {submeshIndex} is below 0", nameof(submeshIndex));
        if (shaderPass < -1) throw new ArgumentException($"{DrawRendererMember}: shader pass {shaderPass} is below -1", nameof(shaderPass));
        Record(() => DrawRendererNow(renderer, material, submeshIndex, shaderPass));
    }

    /// <summary>The draw as the buffer runs it: the renderer's mesh, matrix and property block as they are now, drawn the way
    /// <see cref="DrawMesh(Mesh, Matrix4x4, Material, int, int, MaterialPropertyBlock)"/> draws.</summary>
    private static void DrawRendererNow(Renderer renderer, Material material, int submeshIndex, int shaderPass)
    {
        if (!renderer) throw new ArgumentException($"{DrawRendererMember}: the renderer was destroyed", nameof(renderer));
        var mesh = renderer.BoundsMesh;
        if (!mesh) throw new ArgumentException($"{DrawRendererMember}: the renderer has no mesh to draw", nameof(renderer));
        var block = BlockOf(renderer);
        DrawNow(mesh!, renderer.localToWorldMatrix, material, (submeshIndex, shaderPass), block, DrawRendererMember, () => 1);
    }

    /// <summary>A copy of the property block set on the renderer, or null when it has none or an empty one.</summary>
    private static PropertyStore? BlockOf(Renderer renderer)
    {
        if (!renderer.HasPropertyBlock()) return null;
        var held = new MaterialPropertyBlock();
        renderer.GetPropertyBlock(held);
        if (held.isEmpty) return null;
        ThrowIfUnsent(held.Properties, DrawRendererMember);
        return CopyOf(held.Properties);
    }
}
