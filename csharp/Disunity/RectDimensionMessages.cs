using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// Sends OnRectTransformDimensionsChange to the scripts that define it when the size of their rectangle changes.
/// The native world resolves a rectangle's size from its own values, its ancestors and the screen, so no write
/// announces a change to a descendant; the engine compares each listener's size when something that can move one
/// has been written, at the two ends of the frame's canvas update. A layout group, text or graphic lays out again
/// on this message, and without it keeps the size it was first laid out at.
/// </summary>
internal static class RectDimensionMessages
{
    private const string Message = "OnRectTransformDimensionsChange";

    private sealed class Listener(MonoBehaviour script)
    {
        internal readonly MonoBehaviour Script = script;
        internal Vector2? Delivered;
    }

    private static readonly List<Listener> Listeners = new();
    private static bool _changed;
    private static bool _flushing;

    /// <summary>Starts watching a script whose type defines the message.</summary>
    internal static void Register(MonoBehaviour script)
    {
        if (!ScriptMessages.Defines(script.GetType(), Message)) return;
        Listeners.Add(new Listener(script));
        _changed = true;
    }

    /// <summary>Notes that a rectangle's size, a canvas, the hierarchy, the screen or an object's activity changed, so
    /// the next flush compares the listeners' sizes.</summary>
    internal static void MarkChanged() => _changed = true;

    internal static void Reset()
    {
        Listeners.Clear();
        _changed = false;
        _flushing = false;
    }

    /// <summary>Sends the message to each active listener whose size differs from the last one it was told. A handler
    /// that changes a size is told on the next flush, and a flush a handler starts does nothing.</summary>
    internal static void Flush()
    {
        if (!_changed || _flushing) return;
        _changed = false;
        _flushing = true;
        try
        {
            foreach (var listener in Listeners.ToArray()) Deliver(listener);
            Listeners.RemoveAll(listener => listener.Script.Destroyed);
        }
        finally { _flushing = false; }
    }

    private static void Deliver(Listener listener)
    {
        var script = listener.Script;
        if (script.Destroyed || !script.gameObject.activeInHierarchy) return;
        if (script.GetComponent<RectTransform>() is not { } rect) return;
        var size = rect.rect.size;
        if (listener.Delivered == size) return;
        listener.Delivered = size;
        ScriptMessages.Invoke(script, Message);
    }
}
