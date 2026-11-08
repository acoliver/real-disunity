using Unity.Collections;

namespace UnityEngine.Rendering;

/// <summary>
/// The handle a render pipeline schedules its work through. <see cref="ExecuteCommandBuffer"/> copies a buffer's
/// commands onto the pending list and <see cref="Submit"/> sends the pending commands to the world in order and empties
/// the list. The list is the engine's single one, so every copy of a context shares it, and the struct holds no data:
/// all values are equal. The renderer list members make lists the host draws. The other members are declared so a pipeline loads
/// and links, but the engine does not draw or validate render passes through a context yet: they throw
/// <see cref="NotSupportedException"/> naming the member.
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

    public void Submit() => SendPending();

    /// <summary>Sends the pending commands to the world in order and empties the list.</summary>
    private static void SendPending()
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

    /// <summary>Whether an active, enabled script defines <c>OnRenderObject</c>, so the pipeline has callbacks to run.</summary>
    public bool HasInvokeOnRenderObjectCallbacks() => RenderObjectScripts().Any();

    /// <summary>Calls <c>OnRenderObject</c> on each active, enabled script that defines it, at once and in no set order (the
    /// engine's choice: nothing draws in between). An exception a script throws is logged against it.</summary>
    public void InvokeOnRenderObjectCallback()
    {
        foreach (var script in RenderObjectScripts().ToList()) Disunity.Hosting.ScriptMessages.Invoke(script, "OnRenderObject");
    }

    private static IEnumerable<MonoBehaviour> RenderObjectScripts() =>
        Object.FindObjectsByType<MonoBehaviour>(FindObjectsSortMode.None)
            .Where(script => script.isActiveAndEnabled && Disunity.Hosting.ScriptMessages.Defines(script.GetType(), "OnRenderObject"));

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

    /// <summary>Makes a list of the renderers a culling result kept that have a pass named in <c>drawSettings</c> (up to the first
    /// empty slot; at least one name), filtered by the queue range and layer masks and sorted by the sorting criteria, and
    /// returns it. The list is made at once, so the context first sends the pending commands, in order, which brings the culling
    /// result into the world. A default culling result, a settings value without a pass name, a queue range whose lower bound is above
    /// its upper bound or an override material that was destroyed throws <see cref="ArgumentException"/>. The list lasts for the
    /// frame that made it.</summary>
    public RendererList CreateRendererList(ref RendererListParams parameters)
    {
        var cull = RequireCulling(parameters.cullingResults, "RendererListParams.cullingResults");
        var drawing = parameters.drawSettings;
        var filtering = parameters.filteringSettings;
        var range = filtering.renderQueueRange;
        var names = PassNames(drawing);
        if (range.lowerBound > range.upperBound)
            throw new ArgumentException($"ScriptableRenderContext.CreateRendererList: the queue range {range.lowerBound} to {range.upperBound} is empty");
        var material = drawing.overrideMaterial;
        if (material is not null && !material)
            throw new ArgumentException("ScriptableRenderContext.CreateRendererList: the override material was destroyed", nameof(parameters));
        var materialId = material is null ? 0UL : material.WorldLink.Id;
        SendPending();
        return new RendererList
        {
            Id = WorldGraphics.CreateDrawList(cull, names, range.lowerBound, range.upperBound, (uint)filtering.layerMask,
                filtering.renderingLayerMask, (uint)drawing.sortingSettings.criteria, (uint)drawing.perObjectData, materialId,
                drawing.overrideMaterialPassIndex),
        };
    }

    /// <summary>Makes a list of the shadow casters a culling result kept for one light, inside the split's culling planes and
    /// sphere, and returns it. The light index is a position among the visible lights and must not be negative. A default
    /// culling result or a negative light index throws <see cref="ArgumentException"/>. As in <see cref="CreateRendererList"/>, the
    /// context sends the pending commands first.</summary>
    public RendererList CreateShadowRendererList(ref ShadowDrawingSettings settings)
    {
        var cull = RequireCulling(settings.cullingResults, "ShadowDrawingSettings.cullingResults");
        if (settings.lightIndex < 0)
            throw new ArgumentOutOfRangeException(nameof(settings), settings.lightIndex, "ScriptableRenderContext.CreateShadowRendererList: the light index is negative");
        var split = settings.splitData;
        var planes = new float[split.cullingPlaneCount * 4];
        for (var index = 0; index < split.cullingPlaneCount; index++)
        {
            var plane = split.GetCullingPlane(index);
            (planes[index * 4], planes[index * 4 + 1], planes[index * 4 + 2], planes[index * 4 + 3]) = (plane.normal.x, plane.normal.y, plane.normal.z, plane.distance);
        }
        SendPending();
        return new RendererList
        {
            Id = WorldGraphics.CreateShadowList(cull, settings.lightIndex, planes, split.cullingSphere, (uint)settings.objectsFilter,
                (uint)settings.projectionType, settings.useRenderingLayerMaskTest),
        };
    }

    /// <summary>An invalid list reports <see cref="RendererListStatus.kRendererListInvalid"/>; every other list reports
    /// <see cref="RendererListStatus.kRendererListPopulated"/>, because the engine cannot know how many renderers a list holds until
    /// the host draws it (the engine's choice).</summary>
    public readonly RendererListStatus QueryRendererListStatus(RendererList rendererList) =>
        rendererList.isValid ? RendererListStatus.kRendererListPopulated : RendererListStatus.kRendererListInvalid;

    /// <summary>Does nothing: the world builds a list when it is made, so there is nothing to prepare in the background.</summary>
    public void PrepareRendererListsAsync(List<RendererList> rendererLists)
    {
    }

    private static ulong RequireCulling(CullingResults results, string member) =>
        results.Handle != 0 ? results.Handle : throw new ArgumentException($"{member} holds no culling result", nameof(results));

    /// <summary>The pass names of the settings up to the first slot that holds no tag.</summary>
    private static string[] PassNames(DrawingSettings settings)
    {
        var names = new List<string>();
        for (var slot = 0; slot < DrawingSettings.maxShaderPasses; slot++)
        {
            var tag = settings.GetShaderPassName(slot);
            if (tag == ShaderTagId.none || tag == default) break;
            names.Add(tag.name);
        }
        return names.Count > 0
            ? names.ToArray()
            : throw new ArgumentException("ScriptableRenderContext.CreateRendererList: the drawing settings name no shader pass");
    }

    private static NotSupportedException Unsupported(string member) =>
        new($"ScriptableRenderContext.{member} is not supported: the engine does not render through a scriptable render context yet");

    public readonly bool Equals(ScriptableRenderContext other) => true;
    public override readonly bool Equals(object? obj) => obj is ScriptableRenderContext;
    public override readonly int GetHashCode() => 0;
    public static bool operator ==(ScriptableRenderContext left, ScriptableRenderContext right) => left.Equals(right);
    public static bool operator !=(ScriptableRenderContext left, ScriptableRenderContext right) => !left.Equals(right);
}
