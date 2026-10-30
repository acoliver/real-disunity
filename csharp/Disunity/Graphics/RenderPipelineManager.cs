namespace UnityEngine.Rendering;

/// <summary>
/// The events a render pipeline raises around rendering. Handlers can be added and removed. The engine does not
/// render through a scriptable pipeline yet, so nothing raises them, and <see cref="currentPipeline"/> is null.
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

    public static RenderPipeline? currentPipeline => null;

    /// <summary>Whether any handler is added to the event, for tests that check add and remove.</summary>
    internal static bool HasHandlers =>
        beginContextRendering != null || endContextRendering != null || beginFrameRendering != null || endFrameRendering != null
        || beginCameraRendering != null || endCameraRendering != null || activeRenderPipelineCreated != null || activeRenderPipelineDisposed != null;
}
