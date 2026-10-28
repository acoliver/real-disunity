namespace UnityEngine;

/// <summary>
/// The main thread's synchronization context, which Unity installs before the first startup
/// hook. Work posted from any thread runs on the main thread once a frame, after
/// <c>Update</c> coroutines and before <c>LateUpdate</c>; work posted while that runs waits
/// for the next frame. <see cref="Send"/> from the main thread runs at once, and from another
/// thread waits until the work has run. Copies share the queue.
/// </summary>
internal sealed class UnitySynchronizationContext : SynchronizationContext
{
    private sealed class WorkQueue
    {
        public readonly object Gate = new();
        public List<(SendOrPostCallback Work, object? State, ManualResetEventSlim? Done)> Items = new();
    }

    private readonly WorkQueue _queue;
    private readonly int _mainThread;

    public UnitySynchronizationContext() : this(new WorkQueue(), Environment.CurrentManagedThreadId) { }

    private UnitySynchronizationContext(WorkQueue queue, int mainThread)
    {
        _queue = queue;
        _mainThread = mainThread;
    }

    public override SynchronizationContext CreateCopy() => new UnitySynchronizationContext(_queue, _mainThread);

    public override void Post(SendOrPostCallback d, object? state)
    {
        ArgumentNullException.ThrowIfNull(d);
        lock (_queue.Gate) _queue.Items.Add((d, state, null));
    }

    public override void Send(SendOrPostCallback d, object? state)
    {
        ArgumentNullException.ThrowIfNull(d);
        if (Environment.CurrentManagedThreadId == _mainThread)
        {
            d(state);
            return;
        }
        using var done = new ManualResetEventSlim();
        lock (_queue.Gate) _queue.Items.Add((d, state, done));
        done.Wait();
    }

    /// <summary>
    /// Runs the work queued before this call, in posted order, with this context current. Work
    /// that throws does not stop the rest; the first exception is rethrown afterwards, so the
    /// frame reports it as a script failure.
    /// </summary>
    internal void Execute()
    {
        List<(SendOrPostCallback Work, object? State, ManualResetEventSlim? Done)> items;
        lock (_queue.Gate)
        {
            if (_queue.Items.Count == 0) return;
            items = _queue.Items;
            _queue.Items = new();
        }
        var previous = Current;
        SetSynchronizationContext(this);
        Exception? failure = null;
        try
        {
            foreach (var (work, state, done) in items)
            {
                try { work(state); }
                catch (Exception error) { failure ??= error; }
                finally { done?.Set(); }
            }
        }
        finally { SetSynchronizationContext(previous); }
        if (failure != null) System.Runtime.ExceptionServices.ExceptionDispatchInfo.Capture(failure).Throw();
    }
}
