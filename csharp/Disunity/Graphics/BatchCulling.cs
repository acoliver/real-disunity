using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>One named value of a batch: the shader property it feeds and where it sits in the batch's buffer.</summary>
public struct MetadataValue
{
    public int NameID;
    public uint Value;
}

/// <summary>What a culling callback is asked to cull for.</summary>
public struct BatchCullingContext
{
    public NativeArray<Plane> cullingPlanes;
    public uint cullingLayerMask;
}

/// <summary>Where a culling callback writes what to draw.</summary>
public struct BatchCullingOutput
{
    public NativeArray<BatchCullingOutputDrawCommands> drawCommands;
}

/// <summary>
/// The draw commands a culling callback produces. The callback allocates the arrays with
/// <c>UnsafeUtility.Malloc</c> and the renderer that consumes them frees them.
/// </summary>
public unsafe struct BatchCullingOutputDrawCommands
{
    public BatchDrawCommand* drawCommands;
    public BatchDrawRange* drawRanges;
    public int* visibleInstances;
    public float* instanceSortingPositions;
    public int drawCommandCount;
    public int drawRangeCount;
    public int visibleInstanceCount;
    public int instanceSortingPositionFloatCount;
}

/// <summary>Draws a run of visible instances of one mesh and material from one batch.</summary>
public struct BatchDrawCommand
{
    public uint visibleOffset;
    public uint visibleCount;
    public BatchID batchID;
    public BatchMaterialID materialID;
    public BatchMeshID meshID;
    public ushort submeshIndex;
    public ushort splitVisibilityMask;
    public BatchDrawCommandFlags flags;
    public int sortingPosition;
}

/// <summary>A run of draw commands that share filter settings.</summary>
public struct BatchDrawRange
{
    public BatchDrawCommandType drawCommandsType;
    public uint drawCommandsBegin;
    public uint drawCommandsCount;
    public BatchFilterSettings filterSettings;
}

/// <summary>Which renderers a draw range applies to. The default is no shadow casting and no shadow receiving.</summary>
public struct BatchFilterSettings
{
    private ShadowCastingMode _shadowCastingMode;
    private bool _receiveShadows;

    public uint renderingLayerMask;

    public ShadowCastingMode shadowCastingMode
    {
        readonly get => _shadowCastingMode;
        set => _shadowCastingMode = value;
    }

    public bool receiveShadows
    {
        readonly get => _receiveShadows;
        set => _receiveShadows = value;
    }
}
