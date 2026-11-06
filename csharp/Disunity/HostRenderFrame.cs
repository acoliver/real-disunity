using UnityEngine;
using UnityEngine.Rendering;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>How many consecutive frames failing with the same exception make the host stop calling a pipeline.</summary>
    private const int RenderFailureLimit = 3;

    /// <summary>What the host remembers about one pipeline's failures: the exceptions it reported, the one the last frame
    /// failed with, how many frames in a row failed that way, and whether the host gave the pipeline up.</summary>
    private sealed class RenderFailures
    {
        public readonly HashSet<string> Reported = new();
        public string? LastKey;
        public int Streak;
        public bool Stopped;
    }

    private static RenderFailures _renderFailures = new();
    private static bool _rendering;

    /// <summary>
    /// Calls the current pipeline's <c>Render</c> once, with the enabled cameras on active objects by depth (see
    /// <see cref="Camera.RenderOrder"/>), after the frame's script phases, the engine's choice. It runs on every step, with an
    /// empty list when no camera is enabled. An exception from game code is reported once per distinct exception (type,
    /// message and top stack frame) and the next step calls Render again, so a fixed member shows at once; after
    /// <see cref="RenderFailureLimit"/> consecutive steps failing with the same exception the host stops calling this
    /// pipeline and says so, so a log does not fill with one exception per frame. A native error fails the step.
    /// </summary>
    private static void RenderFrame()
    {
        if (_faulted || RenderPipelineManager.currentPipeline is not { } pipeline || _renderFailures.Stopped) return;
        try
        {
            RunRender(pipeline, Camera.RenderOrder());
            _renderFailures.LastKey = null;
            _renderFailures.Streak = 0;
        }
        catch (Exception error) when (IsGameException(error)) { NoteRenderFailure(pipeline, error); }
    }

    private static void NoteRenderFailure(RenderPipeline pipeline, Exception error)
    {
        var state = _renderFailures;
        var key = $"{error.GetType().FullName}\n{error.Message}\n{error.StackTrace?.Split('\n', 2)[0].Trim()}";
        state.Streak = key == state.LastKey ? state.Streak + 1 : 1;
        state.LastKey = key;
        if (state.Reported.Add(key)) ReportScriptException(error, null);
        if (state.Streak < RenderFailureLimit) return;
        state.Stopped = true;
        Debug.LogError($"{pipeline.GetType().Name}.Render failed in {RenderFailureLimit} consecutive frames with the same exception, "
            + "so the engine stopped calling it: " + error.GetType().Name + ": " + error.Message);
    }

    /// <summary>Renders one camera through the pipeline when the host is calling one, for <see cref="Camera.Render"/>; false
    /// when there is none (or the host gave it up). Throws <see cref="InvalidOperationException"/> when a pipeline's Render is
    /// already running. An exception from the pipeline reaches the caller.</summary>
    internal static bool TryRenderThroughPipeline(Camera camera)
    {
        if (RenderPipelineManager.currentPipeline is not { } pipeline || _renderFailures.Stopped) return false;
        if (_rendering) throw new InvalidOperationException("Camera.Render was called from inside a render pipeline's Render");
        RunRender(pipeline, [camera]);
        return true;
    }

    private static void RunRender(RenderPipeline pipeline, List<Camera> cameras)
    {
        var context = default(ScriptableRenderContext);
        _rendering = true;
        try
        {
            RenderPipelineManager.RaiseBeginContext(context, cameras);
            pipeline.InvokeRender(context, cameras);
            RenderPipelineManager.RaiseEndContext(context, cameras);
        }
        finally
        {
            _rendering = false;
            ScriptableRenderContext.DiscardPending();
        }
    }
}
