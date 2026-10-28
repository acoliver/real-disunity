using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly object ScriptExceptionGate = new();
    private static int _scriptExceptions;
    private static string? _firstScriptException;

    /// <summary>How many exceptions thrown by game code the engine has logged since the world
    /// loaded. The play bridge reports it when a run ends.</summary>
    public static int ScriptExceptionCount
    {
        get { lock (ScriptExceptionGate) return _scriptExceptions; }
    }

    /// <summary>The first logged script exception since the world loaded, as the log shows it
    /// (type and message), or null.</summary>
    public static string? FirstScriptException
    {
        get { lock (ScriptExceptionGate) return _firstScriptException; }
    }

    /// <summary>The end-of-run line for tooling, or null when no script exception was logged.</summary>
    public static string? ScriptExceptionSummary => ScriptExceptionCount is var count and > 0
        ? $"{count} script exceptions were logged; the first: {FirstScriptException}"
        : null;

    /// <summary>
    /// Whether an exception that reached the engine from game code is the game's own. A
    /// <see cref="DisunityException"/> is a native error that surfaced through an engine call
    /// the game made; it leaves the world in an unknown state and fails the step.
    /// </summary>
    internal static bool IsGameException(Exception exception) => exception is not DisunityException;

    /// <summary>
    /// Logs an exception thrown by game code with <see cref="Debug.LogException(Exception, UnityEngine.Object)"/>
    /// against <paramref name="script"/> and counts it. Unity does this for every message,
    /// coroutine step and posted callback, and the frame goes on. Engine errors do not come
    /// here; they fail the step.
    /// </summary>
    internal static void ReportScriptException(Exception exception, UnityEngine.Object? script)
    {
        lock (ScriptExceptionGate)
        {
            _scriptExceptions++;
            _firstScriptException ??= DebugLogHandler.Describe(exception);
        }
        Debug.LogException(exception, script);
    }

    private static void ResetScriptExceptions()
    {
        lock (ScriptExceptionGate)
        {
            _scriptExceptions = 0;
            _firstScriptException = null;
        }
    }
}
