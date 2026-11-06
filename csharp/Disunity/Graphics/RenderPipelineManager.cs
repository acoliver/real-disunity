namespace UnityEngine.Rendering;

/// <summary>
/// The events a render pipeline raises, and the live pipeline. The host creates the pipeline from the active asset
/// and raises <see cref="activeRenderPipelineCreated"/> and <see cref="activeRenderPipelineDisposed"/> around it. The
/// host raises <see cref="beginContextRendering"/> and <see cref="endContextRendering"/> around each call of the pipeline's
/// Render; the camera and frame events are raised by the pipeline's own calls of the matching RenderPipeline helpers.
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

    internal static void RaiseBeginContext(ScriptableRenderContext context, List<Camera> cameras) => beginContextRendering?.Invoke(context, cameras);
    internal static void RaiseEndContext(ScriptableRenderContext context, List<Camera> cameras) => endContextRendering?.Invoke(context, cameras);
    internal static void RaiseBeginCamera(ScriptableRenderContext context, Camera camera) => beginCameraRendering?.Invoke(context, camera);
    internal static void RaiseEndCamera(ScriptableRenderContext context, Camera camera) => endCameraRendering?.Invoke(context, camera);
    internal static void RaiseBeginFrame(ScriptableRenderContext context, Camera[] cameras) => beginFrameRendering?.Invoke(context, cameras);
    internal static void RaiseEndFrame(ScriptableRenderContext context, Camera[] cameras) => endFrameRendering?.Invoke(context, cameras);

    /// <summary>Whether any handler is added to the event, for tests that check add and remove.</summary>
    internal static bool HasHandlers =>
        beginContextRendering != null || endContextRendering != null || beginFrameRendering != null || endFrameRendering != null
        || beginCameraRendering != null || endCameraRendering != null || activeRenderPipelineCreated != null || activeRenderPipelineDisposed != null;
}
