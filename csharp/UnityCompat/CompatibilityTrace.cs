using System.Diagnostics;

namespace UnityEngine;

public static class CompatibilityTrace
{
    private const int LineLimit = 4096;
    private static readonly long Origin = Stopwatch.GetTimestamp();
    private static int _lines;
    private static int _reads;
    private static int _failures;
    private static int _events;
    public static bool Enabled { get; } = Environment.GetEnvironmentVariable("COMPAT_TRACE") == "1";

    public static Scope Begin(string category, string operation)
    {
        if (!Enabled) return default;
        if (category == "native" && operation is "object.get" or "component.get" or "object.alive" or "component.alive" or "component.list" or "transform.get" or "time.get")
        {
            var read = Interlocked.Increment(ref _reads);
            if (read > 32 && read % 128 != 0) return default;
        }
        Write($"begin {category} {operation}");
        return new Scope(category, operation, Stopwatch.GetTimestamp());
    }

    public static void Event(string category, string value)
    {
        if (!Enabled || Interlocked.Increment(ref _events) > 128) return;
        Console.Error.WriteLine($"compat.trace event {category} {value}");
    }

    public static void Failure(string request, string code, string message)
    {
        if (!Enabled || Interlocked.Increment(ref _failures) > 8) return;
        Console.Error.WriteLine($"compat.trace failure {request} [{code}] {message}{Environment.NewLine}{Environment.StackTrace}");
    }

    private static void Write(string message)
    {
        var line = Interlocked.Increment(ref _lines);
        if (line > LineLimit) return;
        Console.WriteLine($"compat.trace {line} {Stopwatch.GetElapsedTime(Origin).TotalMilliseconds:F3}ms {message}");
        if (line == LineLimit) Console.WriteLine("compat.trace limit reached; further trace output suppressed");
    }

    public readonly struct Scope(string category, string operation, long started) : IDisposable
    {
        public void Dispose()
        {
            if (started != 0) Write($"end {category} {operation} elapsed_ms={Stopwatch.GetElapsedTime(started).TotalMilliseconds:F3}");
        }
    }
}
