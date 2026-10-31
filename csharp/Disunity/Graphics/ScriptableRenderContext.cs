using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// The handle a render pipeline schedules its work through. The members are declared so a pipeline loads and
/// links, but the engine does not render through a context yet: every member that would cull, record, draw or
/// submit throws <see cref="NotSupportedException"/> naming the member. The struct holds no data, so all values
/// are equal.
/// </summary>
public struct ScriptableRenderContext : IEquatable<ScriptableRenderContext>
{
    public void Submit() => throw Unsupported("Submit");
    public readonly bool SubmitForRenderPassValidation() => throw Unsupported("SubmitForRenderPassValidation");
    public void ExecuteCommandBuffer(CommandBuffer commandBuffer) => throw Unsupported("ExecuteCommandBuffer");
    public void ExecuteCommandBufferAsync(CommandBuffer commandBuffer, ComputeQueueType queueType) => throw Unsupported("ExecuteCommandBufferAsync");
    public CullingResults Cull(ref ScriptableCullingParameters parameters) => throw Unsupported("Cull");
    public void SetupCameraProperties(Camera camera, bool stereoSetup = false) => throw Unsupported("SetupCameraProperties");
    public void DrawSkybox(Camera camera) => throw Unsupported("DrawSkybox");
    public void DrawGizmos(Camera camera, GizmoSubset gizmoSubset) => throw Unsupported("DrawGizmos");
    public void DrawUIOverlay(Camera camera) => throw Unsupported("DrawUIOverlay");
    public void DrawWireOverlay(Camera camera) => throw Unsupported("DrawWireOverlay");
    public void InvokeOnRenderObjectCallback() => throw Unsupported("InvokeOnRenderObjectCallback");

    public void DrawRenderers(CullingResults cullingResults, ref DrawingSettings drawingSettings, ref FilteringSettings filteringSettings) =>
        throw Unsupported("DrawRenderers");

    public void DrawRenderers(CullingResults cullingResults, ref DrawingSettings drawingSettings, ref FilteringSettings filteringSettings,
        ref RenderStateBlock stateBlock) => throw Unsupported("DrawRenderers");

    public void DrawRenderers(CullingResults cullingResults, ref DrawingSettings drawingSettings, ref FilteringSettings filteringSettings,
        NativeArray<ShaderTagId> renderTypes, NativeArray<RenderStateBlock> stateBlocks) => throw Unsupported("DrawRenderers");

    public void BeginRenderPass(int width, int height, int samples, NativeArray<AttachmentDescriptor> attachments, int depthAttachmentIndex = -1) =>
        throw Unsupported("BeginRenderPass");

    public void BeginSubPass(NativeArray<int> colors, bool isDepthStencilReadOnly = false) => throw Unsupported("BeginSubPass");
    public void BeginSubPass(NativeArray<int> colors, NativeArray<int> inputs, bool isDepthStencilReadOnly = false) => throw Unsupported("BeginSubPass");
    public void EndSubPass() => throw Unsupported("EndSubPass");
    public void EndRenderPass() => throw Unsupported("EndRenderPass");

    public RendererList CreateRendererList(ref RendererListParams parameters) => throw Unsupported("CreateRendererList");
    public readonly RendererListStatus QueryRendererListStatus(RendererList rendererList) => throw Unsupported("QueryRendererListStatus");
    public void PrepareRendererListsAsync(List<RendererList> rendererLists) => throw Unsupported("PrepareRendererListsAsync");

    private static NotSupportedException Unsupported(string member) =>
        new($"ScriptableRenderContext.{member} is not supported: the engine does not render through a scriptable render context yet");

    public readonly bool Equals(ScriptableRenderContext other) => true;
    public override readonly bool Equals(object? obj) => obj is ScriptableRenderContext;
    public override readonly int GetHashCode() => 0;
    public static bool operator ==(ScriptableRenderContext left, ScriptableRenderContext right) => left.Equals(right);
    public static bool operator !=(ScriptableRenderContext left, ScriptableRenderContext right) => !left.Equals(right);
}

/// <summary>
/// A list of renderers to draw. The engine builds none (<see cref="ScriptableRenderContext.CreateRendererList"/>
/// throws), so a value is never valid.
/// </summary>
public struct RendererList : IEquatable<RendererList>
{
    public static RendererList nullRendererList => default;

    public readonly bool isValid => false;

    public readonly bool Equals(RendererList other) => true;
    public override readonly bool Equals(object? obj) => obj is RendererList;
    public override readonly int GetHashCode() => 0;
    public static bool operator ==(RendererList left, RendererList right) => left.Equals(right);
    public static bool operator !=(RendererList left, RendererList right) => !left.Equals(right);
}

/// <summary>What a renderer list is made from. Plain data; nothing builds a list from it yet.</summary>
public struct RendererListParams
{
    public CullingResults cullingResults;
    public DrawingSettings drawSettings;
    public FilteringSettings filteringSettings;
    public ShaderTagId tagName;
    public bool isPassTagName;
    public NativeArray<ShaderTagId> tagValues;
    public NativeArray<RenderStateBlock> stateBlocks;

    public RendererListParams(CullingResults cullingResults, DrawingSettings drawSettings, FilteringSettings filteringSettings)
    {
        this.cullingResults = cullingResults;
        this.drawSettings = drawSettings;
        this.filteringSettings = filteringSettings;
        tagName = ShaderTagId.none;
        isPassTagName = false;
        tagValues = default;
        stateBlocks = default;
    }

    public readonly bool isValid => false;
}
