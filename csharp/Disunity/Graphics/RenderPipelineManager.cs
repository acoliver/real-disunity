namespace UnityEngine.Rendering;

/// <summary>
/// The events a render pipeline raises, and the live pipeline. The host creates the pipeline from the active asset
/// and raises <see cref="activeRenderPipelineCreated"/> and <see cref="activeRenderPipelineDisposed"/> around it. The
/// engine does not render through the pipeline yet, so the rendering events are never raised.
/// </summary>
public static class RenderPipelineManager
{
    public static event Action<ScriptableRenderContext, List<Camera>>? beginContextRendering;
    public static event Action<ScriptableRenderContext, List<Camera>>? endContextRendering;
    public static event Action<ScriptableRenderContext, Camera[]>? beginFrameRendering;
    public static event Action<ScriptableRenderContext, Camera[]>? endFrameRendering;
    public static event Action<ScriptableRenderContext, Camera>? beginCameraRendering;
    public static event Action<ScriptableRenderContext, Camera>? endCameraRendering;
    public static event Action? activeRenderPipelineCreated;
    public static event Action? activeRenderPipelineDisposed;

    private static RenderPipeline? _current;

    /// <summary>The pipeline the host created from the active asset; null with no active asset, before the first
    /// frame step creates it, and when the asset's <c>CreatePipeline</c> threw.</summary>
    public static RenderPipeline? currentPipeline => Volatile.Read(ref _current);

    /// <summary>Makes a freshly created pipeline current and raises <see cref="activeRenderPipelineCreated"/>.</summary>
    internal static void Activate(RenderPipeline pipeline)
    {
        Volatile.Write(ref _current, pipeline);
        activeRenderPipelineCreated?.Invoke();
    }

    /// <summary>Clears <see cref="currentPipeline"/> and returns the pipeline it held.</summary>
    internal static RenderPipeline? Deactivate() => Interlocked.Exchange(ref _current, null);

    /// <summary>Raises <see cref="activeRenderPipelineDisposed"/> after a pipeline was disposed.</summary>
    internal static void RaiseDisposed() => activeRenderPipelineDisposed?.Invoke();

    /// <summary>Whether any handler is added to the event, for tests that check add and remove.</summary>
    internal static bool HasHandlers =>
        beginContextRendering != null || endContextRendering != null || beginFrameRendering != null || endFrameRendering != null
        || beginCameraRendering != null || endCameraRendering != null || activeRenderPipelineCreated != null || activeRenderPipelineDisposed != null;
}
