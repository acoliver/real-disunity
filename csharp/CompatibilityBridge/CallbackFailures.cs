using System.Text.Json;
using UnityEngine;

namespace CompatibilityBridge;

internal sealed class CallbackFailures
{
    private const int RecordLimit = 4096;
    private const int PageLimit = 32;
    private readonly List<object> _records = new();
    public long Total { get; private set; }
    public void Record(MonoBehaviour instance, string callback, string phase, Exception error)
    {
        Total++;
        if (_records.Count == RecordLimit) return;
        _records.Add(new
        {
            sequence = Total, component = instance.NativeHandle,
            script_type = Clip(instance.GetType().FullName, 256), callback = Clip(callback, 64),
            phase = Clip(NativePhaseScope.Phase ?? phase, 64), frame = NativePhaseScope.Frame,
            exception_type = Clip(error.GetType().FullName, 256), message = Clip(error.Message, 4096),
            stack = Clip(error.StackTrace, 8192), native_code = error is NativeWorldException native ? Clip(native.Code, 256) : null
        });
    }
    public object Observe(JsonElement query)
    {
        var after = query.TryGetProperty("after", out var cursor) ? cursor.GetInt32() : 0;
        var limit = query.TryGetProperty("limit", out var count) ? count.GetInt32() : PageLimit;
        if (after < 0 || after > RecordLimit || limit < 1 || limit > PageLimit)
            throw new InvalidDataException("managed.failures requires after 0..4096 and limit 1..32");
        var records = _records.Skip(after).Take(limit).ToArray();
        return new { version = 1, ok = true, data = new
        {
            total = Total, retained = _records.Count, dropped = Total - _records.Count,
            records, next = after + records.Length, has_more = after + records.Length < _records.Count
        } };
    }
    private static string Clip(string? value, int limit) => value is null ? string.Empty : value.Length <= limit ? value : value[..limit];
}

public sealed partial class ScriptSession
{
    private readonly CallbackFailures _failures = new();
    private string _phase = "Initialize";
    public long CallbackFailureCount => _failures.Total;
    private object? Invoke(ScriptState state, string message, object? argument = null, Action? completed = null) =>
        _messages.Invoke(state.Instance, message, argument, completed,
            error => _failures.Record(state.Instance, message, _phase, error));
    private IDisposable EnterPhase(string phase)
    {
        var previous = _phase;
        _phase = phase;
        return new PhaseScope(() => _phase = previous);
    }
    private sealed class PhaseScope(Action restore) : IDisposable
    {
        public void Dispose() => restore();
    }
}
