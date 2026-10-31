using UnityEngine.Rendering;

namespace UnityEngine;

public static partial class Graphics
{
    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera = null,
        int submeshIndex = 0, MaterialPropertyBlock? properties = null, bool castShadows = true, bool receiveShadows = true,
        bool useLightProbes = true) => throw Unsupported(nameof(DrawMesh));

    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera = null, int submeshIndex = 0,
        MaterialPropertyBlock? properties = null, bool castShadows = true, bool receiveShadows = true, bool useLightProbes = true) =>
        throw Unsupported(nameof(DrawMesh));

    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera,
        int submeshIndex, MaterialPropertyBlock? properties, ShadowCastingMode castShadows, bool receiveShadows = true,
        Transform? probeAnchor = null, bool useLightProbes = true) => throw Unsupported(nameof(DrawMesh));

    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties, ShadowCastingMode castShadows, bool receiveShadows = true, Transform? probeAnchor = null,
        bool useLightProbes = true) => throw Unsupported(nameof(DrawMesh));

    public static void DrawMesh(Mesh mesh, Vector3 position, Quaternion rotation, Material material, int layer, Camera? camera,
        int submeshIndex, MaterialPropertyBlock? properties, ShadowCastingMode castShadows, bool receiveShadows, Transform? probeAnchor,
        LightProbeUsage lightProbeUsage, LightProbeProxyVolume? lightProbeProxyVolume = null) => throw Unsupported(nameof(DrawMesh));

    public static void DrawMesh(Mesh mesh, Matrix4x4 matrix, Material material, int layer, Camera? camera, int submeshIndex,
        MaterialPropertyBlock? properties, ShadowCastingMode castShadows, bool receiveShadows, Transform? probeAnchor,
        LightProbeUsage lightProbeUsage, LightProbeProxyVolume? lightProbeProxyVolume = null) => throw Unsupported(nameof(DrawMesh));

    public static void DrawMeshNow(Mesh mesh, Vector3 position, Quaternion rotation) => throw Unsupported(nameof(DrawMeshNow));
    public static void DrawMeshNow(Mesh mesh, Vector3 position, Quaternion rotation, int materialIndex) => throw Unsupported(nameof(DrawMeshNow));
    public static void DrawMeshNow(Mesh mesh, Matrix4x4 matrix) => throw Unsupported(nameof(DrawMeshNow));
    public static void DrawMeshNow(Mesh mesh, Matrix4x4 matrix, int materialIndex) => throw Unsupported(nameof(DrawMeshNow));

    public static void DrawMeshInstanced(Mesh mesh, int submeshIndex, Material material, Matrix4x4[] matrices, int count = -1,
        MaterialPropertyBlock? properties = null, ShadowCastingMode castShadows = ShadowCastingMode.On, bool receiveShadows = true,
        int layer = 0, Camera? camera = null, LightProbeUsage lightProbeUsage = LightProbeUsage.BlendProbes,
        LightProbeProxyVolume? lightProbeProxyVolume = null) => throw Unsupported(nameof(DrawMeshInstanced));

    public static void DrawMeshInstanced(Mesh mesh, int submeshIndex, Material material, List<Matrix4x4> matrices,
        MaterialPropertyBlock? properties = null, ShadowCastingMode castShadows = ShadowCastingMode.On, bool receiveShadows = true,
        int layer = 0, Camera? camera = null, LightProbeUsage lightProbeUsage = LightProbeUsage.BlendProbes,
        LightProbeProxyVolume? lightProbeProxyVolume = null) => throw Unsupported(nameof(DrawMeshInstanced));

    public static void DrawMeshInstancedProcedural(Mesh mesh, int submeshIndex, Material material, Bounds bounds, int count,
        MaterialPropertyBlock? properties = null, ShadowCastingMode castShadows = ShadowCastingMode.On, bool receiveShadows = true,
        int layer = 0, Camera? camera = null, LightProbeUsage lightProbeUsage = LightProbeUsage.BlendProbes,
        LightProbeProxyVolume? lightProbeProxyVolume = null) => throw Unsupported(nameof(DrawMeshInstancedProcedural));

    public static void DrawMeshInstancedIndirect(Mesh mesh, int submeshIndex, Material material, Bounds bounds, ComputeBuffer bufferWithArgs,
        int argsOffset = 0, MaterialPropertyBlock? properties = null, ShadowCastingMode castShadows = ShadowCastingMode.On,
        bool receiveShadows = true, int layer = 0, Camera? camera = null, LightProbeUsage lightProbeUsage = LightProbeUsage.BlendProbes,
        LightProbeProxyVolume? lightProbeProxyVolume = null) => throw Unsupported(nameof(DrawMeshInstancedIndirect));

    public static void DrawMeshInstancedIndirect(Mesh mesh, int submeshIndex, Material material, Bounds bounds, GraphicsBuffer bufferWithArgs,
        int argsOffset = 0, MaterialPropertyBlock? properties = null, ShadowCastingMode castShadows = ShadowCastingMode.On,
        bool receiveShadows = true, int layer = 0, Camera? camera = null, LightProbeUsage lightProbeUsage = LightProbeUsage.BlendProbes,
        LightProbeProxyVolume? lightProbeProxyVolume = null) => throw Unsupported(nameof(DrawMeshInstancedIndirect));

    public static void DrawProcedural(Material material, Bounds bounds, MeshTopology topology, int vertexCount, int instanceCount = 1,
        Camera? camera = null, MaterialPropertyBlock? properties = null, ShadowCastingMode castShadows = ShadowCastingMode.Off,
        bool receiveShadows = true, int layer = 0) => throw Unsupported(nameof(DrawProcedural));

    public static void DrawProcedural(Material material, Bounds bounds, MeshTopology topology, GraphicsBuffer indexBuffer, int indexCount,
        int instanceCount = 1, Camera? camera = null, MaterialPropertyBlock? properties = null,
        ShadowCastingMode castShadows = ShadowCastingMode.Off, bool receiveShadows = true, int layer = 0) =>
        throw Unsupported(nameof(DrawProcedural));

    public static void DrawProceduralNow(MeshTopology topology, int vertexCount, int instanceCount = 1) =>
        throw Unsupported(nameof(DrawProceduralNow));

    public static void DrawProceduralNow(MeshTopology topology, GraphicsBuffer indexBuffer, int indexCount, int instanceCount = 1) =>
        throw Unsupported(nameof(DrawProceduralNow));

    public static void DrawProceduralIndirect(Material material, Bounds bounds, MeshTopology topology, ComputeBuffer bufferWithArgs,
        int argsOffset = 0, Camera? camera = null, MaterialPropertyBlock? properties = null,
        ShadowCastingMode castShadows = ShadowCastingMode.Off, bool receiveShadows = true, int layer = 0) =>
        throw Unsupported(nameof(DrawProceduralIndirect));

    public static void DrawProceduralIndirect(Material material, Bounds bounds, MeshTopology topology, GraphicsBuffer bufferWithArgs,
        int argsOffset = 0, Camera? camera = null, MaterialPropertyBlock? properties = null,
        ShadowCastingMode castShadows = ShadowCastingMode.Off, bool receiveShadows = true, int layer = 0) =>
        throw Unsupported(nameof(DrawProceduralIndirect));

    public static void DrawProceduralIndirect(Material material, Bounds bounds, MeshTopology topology, GraphicsBuffer indexBuffer,
        GraphicsBuffer bufferWithArgs, int argsOffset = 0, Camera? camera = null, MaterialPropertyBlock? properties = null,
        ShadowCastingMode castShadows = ShadowCastingMode.Off, bool receiveShadows = true, int layer = 0) =>
        throw Unsupported(nameof(DrawProceduralIndirect));

    public static void DrawProceduralIndirectNow(MeshTopology topology, ComputeBuffer bufferWithArgs, int argsOffset = 0) =>
        throw Unsupported(nameof(DrawProceduralIndirectNow));

    public static void DrawProceduralIndirectNow(MeshTopology topology, GraphicsBuffer bufferWithArgs, int argsOffset = 0) =>
        throw Unsupported(nameof(DrawProceduralIndirectNow));

    public static void DrawProceduralIndirectNow(MeshTopology topology, GraphicsBuffer indexBuffer, GraphicsBuffer bufferWithArgs,
        int argsOffset = 0) => throw Unsupported(nameof(DrawProceduralIndirectNow));
}
