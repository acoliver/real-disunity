using Unity.Collections;
using UnityEngine.Rendering;

namespace UnityEngine;

public static partial class Graphics
{
    public static void RenderMesh(in RenderParams rparams, Mesh mesh, int submeshIndex, Matrix4x4 objectToWorld,
        Matrix4x4? prevObjectToWorld = null) => throw Unsupported(nameof(RenderMesh));

    public static void RenderMeshInstanced<T>(in RenderParams rparams, Mesh mesh, int submeshIndex, T[] instanceData, int instanceCount = -1,
        int startInstance = 0) where T : unmanaged => throw Unsupported(nameof(RenderMeshInstanced));

    public static void RenderMeshInstanced<T>(in RenderParams rparams, Mesh mesh, int submeshIndex, List<T> instanceData,
        int instanceCount = -1, int startInstance = 0) where T : unmanaged => throw Unsupported(nameof(RenderMeshInstanced));

    public static void RenderMeshInstanced<T>(in RenderParams rparams, Mesh mesh, int submeshIndex, NativeArray<T> instanceData,
        int instanceCount = -1, int startInstance = 0) where T : unmanaged => throw Unsupported(nameof(RenderMeshInstanced));

    public static void RenderMeshIndirect(in RenderParams rparams, Mesh mesh, GraphicsBuffer commandBuffer, int commandCount = 1,
        int startCommand = 0) => throw Unsupported(nameof(RenderMeshIndirect));

    public static void RenderMeshPrimitives(in RenderParams rparams, Mesh mesh, int submeshIndex, int instanceCount = 1) =>
        throw Unsupported(nameof(RenderMeshPrimitives));

    public static void RenderPrimitives(in RenderParams rparams, MeshTopology topology, int vertexCount, int instanceCount = 1) =>
        throw Unsupported(nameof(RenderPrimitives));

    public static void RenderPrimitivesIndexed(in RenderParams rparams, MeshTopology topology, GraphicsBuffer indexBuffer, int indexCount,
        int startIndex = 0, int instanceCount = 1) => throw Unsupported(nameof(RenderPrimitivesIndexed));

    public static void RenderPrimitivesIndirect(in RenderParams rparams, MeshTopology topology, GraphicsBuffer commandBuffer,
        int commandCount = 1, int startCommand = 0) => throw Unsupported(nameof(RenderPrimitivesIndirect));

    public static void RenderPrimitivesIndexedIndirect(in RenderParams rparams, MeshTopology topology, GraphicsBuffer indexBuffer,
        GraphicsBuffer commandBuffer, int commandCount = 1, int startIndex = 0, int startCommand = 0) =>
        throw Unsupported(nameof(RenderPrimitivesIndexedIndirect));
}
