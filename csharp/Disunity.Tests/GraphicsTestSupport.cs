using UnityEngine;

/// <summary>Assertions and a log recorder shared by the graphics object tests.</summary>
internal static class GraphicsTestSupport
{
    /// <summary>Collects what is logged while it is installed, so a test can assert on warnings and errors.</summary>
    internal sealed class LogRecorder : ILogHandler, IDisposable
    {
        private readonly ILogHandler _previous = Debug.unityLogger.logHandler;
        public List<string> Messages { get; } = new();

        public LogRecorder() => Debug.unityLogger.logHandler = this;

        public void LogFormat(LogType logType, UnityEngine.Object? context, string format, params object?[] args) =>
            Messages.Add($"{logType}: {string.Format(format, args)}");

        public void LogException(Exception exception, UnityEngine.Object? context) => Messages.Add($"Exception: {exception.Message}");

        public void Dispose() => Debug.unityLogger.logHandler = _previous;
    }

    public static void Ensure(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    public static void Near(Color expected, Color actual, float tolerance, string what) =>
        Ensure(MathF.Abs(expected.r - actual.r) <= tolerance && MathF.Abs(expected.g - actual.g) <= tolerance
            && MathF.Abs(expected.b - actual.b) <= tolerance && MathF.Abs(expected.a - actual.a) <= tolerance,
            $"{what}: expected {expected}, got {actual}");

    /// <summary>The action must throw <typeparamref name="T"/>, and the message must contain every fragment.</summary>
    public static void Throws<T>(Action action, string what, params string[] fragments) where T : Exception
    {
        try { action(); }
        catch (Exception error) when (error.GetType() == typeof(T))
        {
            foreach (var fragment in fragments)
                Ensure(error.Message.Contains(fragment), $"{what}: message '{error.Message}' lacks '{fragment}'");
            return;
        }
        catch (Exception error)
        {
            throw new InvalidOperationException($"{what}: expected {typeof(T).Name}, got {error.GetType().Name}: {error.Message}");
        }
        throw new InvalidOperationException($"{what}: expected {typeof(T).Name}, nothing was thrown");
    }

    public static void NotSupported(Action action, string what, string member) =>
        Throws<NotSupportedException>(action, what, member, "not supported yet");
}
