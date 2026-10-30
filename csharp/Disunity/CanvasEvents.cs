using Disunity.Hosting;

namespace UnityEngine;

public sealed partial class Canvas
{
    public delegate void WillRenderCanvases();

    /// <summary>Raised once per frame after LateUpdate and before the frame is drawn, after <see cref="preWillRenderCanvases"/>.</summary>
    public static event WillRenderCanvases? willRenderCanvases;

    public static event WillRenderCanvases? preWillRenderCanvases;

    /// <summary>Raises both events now, as the engine does each frame.</summary>
    public static void ForceUpdateCanvases() => RaiseWillRender();

    internal static void RaiseWillRender()
    {
        Raise(preWillRenderCanvases);
        Raise(willRenderCanvases);
    }

    internal static void ClearHandlers() => (willRenderCanvases, preWillRenderCanvases) = (null, null);

    /// <summary>Calls each handler in turn. One that throws is reported as a script exception and the rest still run.</summary>
    private static void Raise(WillRenderCanvases? handlers)
    {
        if (handlers is null) return;
        foreach (var handler in handlers.GetInvocationList().Cast<WillRenderCanvases>())
        {
            try { handler(); }
            catch (Exception error) when (DisunityHost.IsGameException(error)) { DisunityHost.ReportScriptException(error, null); }
        }
    }
}
