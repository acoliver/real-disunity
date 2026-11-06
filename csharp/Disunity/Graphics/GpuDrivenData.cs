using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>Per mesh level of detail information for GPU driven rendering. The engine does not drive renderers from the GPU; nothing reads these.</summary>
public struct GPUDrivenMeshLodInfo
{
    public int lodCount;
    public bool lodSlicingEnabled;
}

/// <summary>The material state GPU driven rendering packs per renderer. The fields are the engine's choice; nothing reads them.</summary>
public struct GPUDrivenPackedMaterialData
{
    public bool isTransparent;
    public bool isMotionVectorsPassEnabled;
    public bool isIndirectSupported;
}

/// <summary>Level of detail groups handed to GPU driven rendering. The fields are the engine's choice; nothing reads them.</summary>
public struct GPUDrivenLODGroupData
{
    public NativeArray<int> lodGroupID;
    public NativeArray<int> lodOffset;
    public NativeArray<int> lodCount;
}

/// <summary>Renderers handed to GPU driven rendering. The fields are the engine's choice; nothing reads them.</summary>
public struct GPUDrivenRendererGroupData
{
    public NativeArray<int> rendererGroupID;
    public NativeArray<Matrix4x4> localToWorldMatrix;
    public NativeArray<GPUDrivenPackedMaterialData> packedMaterialData;
}

/// <summary>Mesh level of detail data per renderer for GPU driven rendering. The fields are the engine's choice; nothing reads them.</summary>
public struct GPUDrivenRendererMeshLodData
{
    public NativeArray<int> rendererGroupID;
    public NativeArray<GPUDrivenMeshLodInfo> meshLodInfo;
}
