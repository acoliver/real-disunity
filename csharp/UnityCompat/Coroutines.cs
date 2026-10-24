using System.Collections;
using CompatibilitySupport;

namespace UnityEngine;

public sealed class WaitForSeconds(float seconds)
{
    public float Seconds { get; } = float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForSecondsRealtime(float seconds)
{
    public float Seconds { get; } = float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForFixedUpdate;
public sealed class WaitForEndOfFrame;
public sealed class Coroutine
{
    public MonoBehaviour Owner { get; }
    internal Stack<IEnumerator> Stack { get; } = new();
    public bool Completed { get; internal set; }
    internal bool Executing { get; set; }
    internal object? Yielded { get; set; }
    internal double ResumeTime { get; set; }
    internal long ResumeFrame { get; set; }
    internal Coroutine(MonoBehaviour owner) { Owner = owner; }
    public Coroutine(MonoBehaviour owner, IEnumerator enumerator) { Owner = owner; Stack.Push(enumerator); }
}

public sealed class CoroutineScheduler
{
    private readonly List<Coroutine> _routines = new();
    public int Count => _routines.Count;
    public Coroutine Start(MonoBehaviour owner, IEnumerator routine)
    {
        ArgumentNullException.ThrowIfNull(routine);
        var result = new Coroutine(owner, routine);
        _routines.Add(result);
        Advance(result);
        return result;
    }
    public void Stop(Coroutine routine)
    {
        routine.Completed = true;
        _routines.Remove(routine);
        var failures = new CleanupFailures();
        if (!routine.Executing)
            while (routine.Stack.TryPop(out var e))
                if (e is IDisposable disposable) failures.Run(disposable.Dispose);
        failures.ThrowIfAny();
    }
    public void Stop(MonoBehaviour owner, IEnumerator? routine = null)
    {
        var failures = new CleanupFailures();
        foreach (var item in _routines.Where(r => ReferenceEquals(r.Owner, owner) && (routine is null || r.Stack.Contains(routine))).ToArray())
            failures.Run(() => Stop(item));
        failures.ThrowIfAny();
    }
    public void Clear()
    {
        var failures = new CleanupFailures();
        foreach (var routine in _routines.ToArray()) failures.Run(() => Stop(routine));
        failures.ThrowIfAny();
    }
    public void Dispatch(string phase) => Dispatch(phase, null);
    internal void Dispatch(string phase, Action<MonoBehaviour, Exception>? failed)
    {
        foreach (var r in _routines.ToArray())
        {
            try { DispatchOne(r, phase); }
            catch (Exception error) when (failed is not null) { failed(r.Owner, error); }
        }
    }
    private void DispatchOne(Coroutine r, string phase)
    {
        if (r.Completed) return;
        if (!r.Owner.IsAlive || !r.Owner.gameObject.activeInHierarchy) { Stop(r); return; }
        if (Ready(r, phase)) Advance(r);
    }
    private static bool Ready(Coroutine r, string phase)
    {
        if (r.Yielded is WaitForFixedUpdate) return phase == "FixedUpdate";
        if (r.Yielded is WaitForEndOfFrame) return phase == "LateUpdate";
        if (phase != "Update") return false;
        using var result = NativeWorld.Request("time.get");
        var t = NativeWorld.Data(result);
        return t.GetProperty("frame").GetInt64() >= r.ResumeFrame && t.GetProperty("time").GetDouble() + 1e-7 >= r.ResumeTime && (r.Yielded is not Coroutine other || other.Completed);
    }
    private void Advance(Coroutine r)
    {
        try
        {
            while (r.Stack.TryPeek(out var e))
            {
                bool more;
                r.Executing = true;
                try { more = e.MoveNext(); }
                finally { r.Executing = false; }
                if (r.Completed) { Stop(r); return; }
                if (!more) { r.Stack.Pop(); (e as IDisposable)?.Dispose(); continue; }
                if (e.Current is IEnumerator nested) { r.Stack.Push(nested); continue; }
                Suspend(r, e.Current);
                return;
            }
            Stop(r);
        }
        catch (Exception error)
        {
            try { Stop(r); }
            catch (Exception cleanupError) { throw new AggregateException(error, cleanupError).Flatten(); }
            throw;
        }
    }
    private static void Suspend(Coroutine r, object? value)
    {
        if (value is not null and not WaitForSeconds and not WaitForFixedUpdate and not WaitForEndOfFrame and not Coroutine)
            throw new NotSupportedException($"Coroutine yield '{value.GetType().FullName}'");
        using var result = NativeWorld.Request("time.get");
        var time = NativeWorld.Data(result);
        r.Yielded = value;
        r.ResumeFrame = time.GetProperty("frame").GetInt64() + 1;
        r.ResumeTime = time.GetProperty("time").GetDouble() + (value is WaitForSeconds wait ? wait.Seconds : 0);
    }
}
