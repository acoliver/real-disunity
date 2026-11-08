using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// The handle a render pipeline schedules its work through. <see cref="ExecuteCommandBuffer"/> copies a buffer's
/// commands onto the pending list and <see cref="Submit"/> sends the pending commands to the world in order and empties
/// the list. The list is the engine's single one, so every copy of a context shares it, and the struct holds no data:
/// all values are equal. The other members are declared so a pipeline loads and links, but the engine does not cull,
/// draw or validate render passes through a context yet: they throw <see cref="NotSupportedException"/> naming the member.
/// </summary>
public struct ScriptableRenderContext : IEquatable<ScriptableRenderContext>
{
    private static readonly List<Action> Pending = new();

    /// <summary>How many commands wait for <see cref="Submit"/>.</summary>
    internal static int PendingCount
    {
        get
        {
            lock (Pending) return Pending.Count;
        }
    }

    /// <summary>Drops the commands that were queued and not submitted. The host calls it when a pipeline's Render
    /// returns, so work a pipeline forgot to submit does not run in a later frame (the engine's choice).</summary>
    internal static void DiscardPending()
    {
        lock (Pending) Pending.Clear();
    }

    public void Submit()
    {
        Action[] batch;
        lock (Pending)
        {
            batch = Pending.ToArray();
            Pending.Clear();
        }
        foreach (var send in batch) send();
    }

    public readonly bool SubmitForRenderPassValidation() => throw Unsupported("SubmitForRenderPassValidation");
    public void ExecuteCommandBuffer(CommandBuffer commandBuffer)
    {
        ArgumentNullException.ThrowIfNull(commandBuffer);
        var commands = commandBuffer.Commands();
        lock (Pending) Pending.AddRange(commands);
    }

    public void ExecuteCommandBufferAsync(CommandBuffer commandBuffer, ComputeQueueType queueType) => throw Unsupported("ExecuteCommandBufferAsync");
    /// <summary>Queues a culling of the world's renderers with the parameters and returns a handle to the result. The record
    /// reaches the world's offscreen list at <see cref="Submit"/>, in order with the commands executed before and after. Parameters
    /// that did not come from <see cref="Camera.TryGetCullingParameters(out ScriptableCullingParameters)"/> throw
    /// <see cref="InvalidOperationException"/>. The result has no visible lights or probes (the engine has no light culling).</summary>
    public CullingResults Cull(ref ScriptableCullingParameters parameters)
    {
        if (parameters.CameraHandle == 0) throw new InvalidOperationException("ScriptableRenderContext.Cull: the parameters did not come from a camera");
        var id = WorldGraphics.NextHandle();
        var copy = parameters;
        lock (Pending) Pending.Add(() => WorldGraphics.RecordCull(id, copy));
        return new CullingResults { Handle = id };
    }
    /// <summary>Queues a record that draws after it use the camera's view and projection matrices and target, read now. The
    /// record reaches the world's offscreen list at <see cref="Submit"/>. A null camera throws <see cref="ArgumentNullException"/>, a
    /// destroyed one <see cref="InvalidOperationException"/>, and <paramref name="stereoSetup"/> <see cref="NotSupportedException"/>
    /// because the engine has no stereo rendering.</summary>
    public void SetupCameraProperties(Camera camera, bool stereoSetup = false)
    {
        ArgumentNullException.ThrowIfNull(camera);
        if (stereoSetup) throw new NotSupportedException("ScriptableRenderContext.SetupCameraProperties: stereo setup is not supported, the engine has no stereo rendering");
        var (view, projection, target) = camera.SetupValues();
        var handle = camera.Handle;
        lock (Pending) Pending.Add(() => WorldGraphics.RecordSetupCamera(handle, view, projection, target));
    }
    /// <summary>Does nothing: it makes canvas geometry available to the camera's culling, and the engine draws canvases
    /// itself from the world's canvas renderers, so the pipeline has no geometry to ask for (the engine's choice).</summary>
    public static void EmitGeometryForCamera(Camera camera) => ArgumentNullException.ThrowIfNull(camera);

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
    public RendererList CreateShadowRendererList(ref ShadowDrawingSettings settings) => throw Unsupported("CreateShadowRendererList");
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
