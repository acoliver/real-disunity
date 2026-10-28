namespace UnityEngine;

/// <summary>
/// The main thread's synchronization context, which Unity installs before the first startup
/// hook. Work posted from any thread runs on the main thread once a frame, after
/// <c>Update</c> coroutines and before <c>LateUpdate</c>; work posted while that runs waits
/// for the next frame. <see cref="Send"/> from the main thread runs at once, and from another
/// thread waits until the work has run. Work that throws is logged and the rest runs, as in
/// Unity. Copies share the queue. Closing the context with its world drops queued work and
/// releases waiting senders.
/// </summary>
internal sealed class UnitySynchronizationContext : SynchronizationContext
{
    private sealed record Work(SendOrPostCallback Callback, object? State, TaskCompletionSource? Done);

    private sealed class WorkQueue
    {
        public readonly object Gate = new();
        public List<Work> Items = new();
        public bool Closed;
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

    /// <summary>Queues work for the next frame; work posted after the world unloads is dropped.</summary>
    public override void Post(SendOrPostCallback d, object? state)
    {
        ArgumentNullException.ThrowIfNull(d);
        lock (_queue.Gate)
            if (!_queue.Closed) _queue.Items.Add(new Work(d, state, null));
    }

    public override void Send(SendOrPostCallback d, object? state)
    {
        ArgumentNullException.ThrowIfNull(d);
        if (Environment.CurrentManagedThreadId == _mainThread)
        {
            d(state);
            return;
        }
        var done = new TaskCompletionSource(TaskCreationOptions.RunContinuationsAsynchronously);
        lock (_queue.Gate)
        {
            ObjectDisposedException.ThrowIf(_queue.Closed, this);
            _queue.Items.Add(new Work(d, state, done));
        }
        done.Task.GetAwaiter().GetResult();
    }

    /// <summary>Runs the work queued before this call, in posted order, with this context
    /// current. Work that throws is logged and does not stop the rest.</summary>
    internal void Execute()
    {
        List<Work> items;
        lock (_queue.Gate)
        {
            if (_queue.Items.Count == 0) return;
            items = _queue.Items;
            _queue.Items = new();
        }
        var previous = Current;
        SetSynchronizationContext(this);
        try
        {
            foreach (var work in items)
            {
                try { work.Callback(work.State); }
                catch (Exception error) { Disunity.Hosting.DisunityHost.ReportScriptException(error, null); }
                finally { work.Done?.TrySetResult(); }
            }
        }
        finally { SetSynchronizationContext(previous); }
    }

    /// <summary>Drops queued work and fails waiting senders with
    /// <see cref="ObjectDisposedException"/>; later posts are dropped and sends fail.</summary>
    internal void Close()
    {
        List<Work> items;
        lock (_queue.Gate)
        {
            _queue.Closed = true;
            items = _queue.Items;
            _queue.Items = new();
        }
        foreach (var work in items)
            work.Done?.TrySetException(new ObjectDisposedException(nameof(UnitySynchronizationContext), "the world unloaded before the work ran"));
    }
}
