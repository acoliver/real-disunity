using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    /// <summary>The message the native scheduler sends the delayed-tasks identity each frame.</summary>
    private const uint DelayedTasksMessage = 14;

    private static UnitySynchronizationContext? _mainThreadContext;
    private static SynchronizationContext? _previousContext;
    private static ulong _delayedTasks;

    /// <summary>Installs the main thread's synchronization context for the loaded world and has
    /// the native scheduler run its work each frame.</summary>
    private static void InstallMainThreadContext()
    {
        _delayedTasks = checked(_nextScript++);
        ThrowIfError(NativeBindings.disunity_world_delayed_tasks(WorldHandle, _delayedTasks, 1));
        _previousContext = SynchronizationContext.Current;
        _mainThreadContext = new UnitySynchronizationContext();
        SynchronizationContext.SetSynchronizationContext(_mainThreadContext);
    }

    /// <summary>Restores the context the thread had before the world loaded. Work still queued
    /// is dropped with the world, and threads waiting in <c>Send</c> are released with an
    /// error.</summary>
    private static void RemoveMainThreadContext()
    {
        if (_mainThreadContext is null) return;
        _mainThreadContext.Close();
        if (ReferenceEquals(SynchronizationContext.Current, _mainThreadContext))
            SynchronizationContext.SetSynchronizationContext(_previousContext);
        _mainThreadContext = null;
        _previousContext = null;
    }

    /// <summary>Whether a native callback is the delayed-tasks phase, which it then runs.</summary>
    private static bool DispatchDelayedTasks(ulong identity, uint message)
    {
        if (message != DelayedTasksMessage || identity != _delayedTasks) return false;
        _mainThreadContext?.Execute();
        return true;
    }
}
