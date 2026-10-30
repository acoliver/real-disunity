using System.Diagnostics;
using System.Globalization;

namespace UnityEngine;

/// <summary>
/// Logging through <see cref="unityLogger"/>. The default handler writes each message to
/// standard output, as a player writes its log, and delivers it to
/// <see cref="Application.logMessageReceived"/> with a stack trace for every type except
/// <see cref="LogType.Log"/>. Debug drawing and the developer console have no display.
/// </summary>
public static class Debug
{
    public static ILogger unityLogger { get; } = new Logger(new DebugLogHandler());
    public static ILogger logger => unityLogger;
    public static bool isDebugBuild => false;
    public static bool developerConsoleEnabled { get; set; }
    public static bool developerConsoleVisible { get; set; }

    public static void Log(object? message) => unityLogger.Log(LogType.Log, message);
    public static void Log(object? message, Object? context) => unityLogger.Log(LogType.Log, message, context);
    public static void LogFormat(string format, params object?[] args) => unityLogger.LogFormat(LogType.Log, format, args);
    public static void LogFormat(Object? context, string format, params object?[] args) => unityLogger.LogFormat(LogType.Log, context, format, args);
    public static void LogFormat(LogType logType, LogOption logOptions, Object? context, string format, params object?[] args) =>
        unityLogger.LogFormat(logType, context, format, args);
    public static void LogWarning(object? message) => unityLogger.Log(LogType.Warning, message);
    public static void LogWarning(object? message, Object? context) => unityLogger.Log(LogType.Warning, message, context);
    public static void LogWarningFormat(string format, params object?[] args) => unityLogger.LogFormat(LogType.Warning, format, args);
    public static void LogWarningFormat(Object? context, string format, params object?[] args) => unityLogger.LogFormat(LogType.Warning, context, format, args);
    public static void LogError(object? message) => unityLogger.Log(LogType.Error, message);
    public static void LogError(object? message, Object? context) => unityLogger.Log(LogType.Error, message, context);
    public static void LogErrorFormat(string format, params object?[] args) => unityLogger.LogFormat(LogType.Error, format, args);
    public static void LogErrorFormat(Object? context, string format, params object?[] args) => unityLogger.LogFormat(LogType.Error, context, format, args);
    public static void LogException(Exception exception) => unityLogger.LogException(exception, null);
    public static void LogException(Exception exception, Object? context) => unityLogger.LogException(exception, context);
    public static void LogAssertion(object? message) => unityLogger.Log(LogType.Assert, message);
    public static void LogAssertion(object? message, Object? context) => unityLogger.Log(LogType.Assert, message, context);

    public static void DrawLine(Vector3 start, Vector3 end) { }
    public static void DrawLine(Vector3 start, Vector3 end, Color color) { }
    public static void DrawLine(Vector3 start, Vector3 end, Color color, float duration) { }
    public static void DrawLine(Vector3 start, Vector3 end, Color color, float duration, bool depthTest) { }
    public static void DrawRay(Vector3 start, Vector3 dir) { }
    public static void DrawRay(Vector3 start, Vector3 dir, Color color) { }
    public static void DrawRay(Vector3 start, Vector3 dir, Color color, float duration) { }
    public static void DrawRay(Vector3 start, Vector3 dir, Color color, float duration, bool depthTest) { }
    public static void Break() { }
    public static void DebugBreak() { }
    public static void ClearDeveloperConsole() { }
}

public interface ILogHandler
{
    void LogFormat(LogType logType, Object? context, string format, params object?[] args);
    void LogException(Exception exception, Object? context);
}

public interface ILogger : ILogHandler
{
    ILogHandler logHandler { get; set; }
    bool logEnabled { get; set; }
    LogType filterLogType { get; set; }
    bool IsLogTypeAllowed(LogType logType);
    void Log(LogType logType, object? message);
    void Log(LogType logType, object? message, Object? context);
    void Log(LogType logType, string tag, object? message);
    void Log(LogType logType, string tag, object? message, Object? context);
    void Log(object? message);
    void Log(string tag, object? message);
    void Log(string tag, object? message, Object? context);
    void LogWarning(string tag, object? message);
    void LogWarning(string tag, object? message, Object? context);
    void LogError(string tag, object? message);
    void LogError(string tag, object? message, Object? context);
    void LogFormat(LogType logType, string format, params object?[] args);
    void LogException(Exception exception);
}

/// <summary>Filters messages by type and passes them to its handler. Types are ordered by
/// severity as Unity orders them: a filter of <see cref="LogType.Warning"/> allows errors,
/// asserts and warnings; exceptions pass every filter while logging is enabled.</summary>
public class Logger(ILogHandler logHandler) : ILogger
{
    public ILogHandler logHandler { get; set; } = logHandler;
    public bool logEnabled { get; set; } = true;
    public LogType filterLogType { get; set; } = LogType.Log;

    public bool IsLogTypeAllowed(LogType logType) =>
        logEnabled && (logType == LogType.Exception || filterLogType == LogType.Exception || logType <= filterLogType);

    public void Log(LogType logType, object? message) => Write(logType, null, null, message);
    public void Log(LogType logType, object? message, Object? context) => Write(logType, null, context, message);
    public void Log(LogType logType, string tag, object? message) => Write(logType, tag, null, message);
    public void Log(LogType logType, string tag, object? message, Object? context) => Write(logType, tag, context, message);
    public void Log(object? message) => Write(LogType.Log, null, null, message);
    public void Log(string tag, object? message) => Write(LogType.Log, tag, null, message);
    public void Log(string tag, object? message, Object? context) => Write(LogType.Log, tag, context, message);
    public void LogWarning(string tag, object? message) => Write(LogType.Warning, tag, null, message);
    public void LogWarning(string tag, object? message, Object? context) => Write(LogType.Warning, tag, context, message);
    public void LogError(string tag, object? message) => Write(LogType.Error, tag, null, message);
    public void LogError(string tag, object? message, Object? context) => Write(LogType.Error, tag, context, message);
    public void LogException(Exception exception) => LogException(exception, null);

    public void LogException(Exception exception, Object? context)
    {
        if (logEnabled) logHandler.LogException(exception, context);
    }

    public void LogFormat(LogType logType, string format, params object?[] args)
    {
        if (IsLogTypeAllowed(logType)) logHandler.LogFormat(logType, null, format, args);
    }

    public void LogFormat(LogType logType, Object? context, string format, params object?[] args)
    {
        if (IsLogTypeAllowed(logType)) logHandler.LogFormat(logType, context, format, args);
    }

    private void Write(LogType logType, string? tag, Object? context, object? message)
    {
        if (!IsLogTypeAllowed(logType)) return;
        // Unity formats numbers and other formattable values with the invariant culture.
        var text = message switch
        {
            null => "Null",
            Object value when !value => "null",
            IFormattable formattable => formattable.ToString(null, CultureInfo.InvariantCulture),
            _ => message.ToString() ?? "",
        };
        logHandler.LogFormat(logType, context, "{0}", tag is null ? text : $"{tag}: {text}");
    }
}

/// <summary>Unity's default handler: writes to standard output and delivers to
/// <see cref="Application"/>'s log events.</summary>
internal sealed class DebugLogHandler : ILogHandler
{
    private static readonly object Gate = new();

    public void LogFormat(LogType logType, Object? context, string format, params object?[] args)
    {
        var message = string.Format(CultureInfo.InvariantCulture, format, args);
        var stack = logType == LogType.Log ? "" : new StackTrace(2, false).ToString();
        Write(message);
        Application.Deliver(message, stack, logType);
    }

    public void LogException(Exception exception, Object? context)
    {
        var message = Describe(exception);
        var stack = StackOf(exception) + InnerOf(exception);
        Write(stack.Length == 0 ? message : message + System.Environment.NewLine + stack);
        Application.Deliver(message, stack, LogType.Exception);
    }

    /// <summary>An exception as Unity's StackTraceUtility describes it: the type's name without
    /// its namespace, then the message when there is one.</summary>
    internal static string Describe(Exception exception) =>
        string.IsNullOrWhiteSpace(exception.Message) ? exception.GetType().Name : $"{exception.GetType().Name}: {exception.Message}";

    /// <summary>An exception's stack trace, or a note when formatting it fails, as it can when
    /// a method in it carries an attribute the engine lacks.</summary>
    /// <summary>Each inner exception after the stack, described and with its own stack, so a failed type
    /// initializer or a wrapped failure names its cause.</summary>
    private static string InnerOf(Exception exception)
    {
        var text = new System.Text.StringBuilder();
        for (var inner = exception.InnerException; inner is not null; inner = inner.InnerException)
        {
            text.Append(System.Environment.NewLine).Append("Inner exception: ").Append(Describe(inner));
            var stack = StackOf(inner);
            if (stack.Length > 0) text.Append(System.Environment.NewLine).Append(stack);
        }
        return text.ToString();
    }

    private static string StackOf(Exception exception)
    {
        try { return exception.StackTrace ?? ""; }
        catch (Exception formatting) { return $"(stack trace unavailable: {formatting.Message})"; }
    }

    private static void Write(string text)
    {
        lock (Gate) Console.Out.WriteLine(text);
    }
}
