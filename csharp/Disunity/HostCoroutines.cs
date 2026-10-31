using System.Collections;
using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly Dictionary<ulong, Coroutine> Routines = new();
    internal static ulong ScriptIdentity(MonoBehaviour owner) => Scripts.Single(entry => ReferenceEquals(entry.Value, owner)).Key;

    internal static unsafe Coroutine StartCoroutine(MonoBehaviour owner, IEnumerator iterator)
    {
        ArgumentNullException.ThrowIfNull(iterator);
        var script = ScriptIdentity(owner);
        ThrowIfError(NativeBindings.disunity_coroutine_ready(WorldHandle, script));
        var routine = new Coroutine(owner, iterator);
        try
        {
            if (!MoveIterator(routine, out var yielded)) return routine;
            var wait = DescribeWait(routine, yielded);
            ulong identity;
            ThrowIfError(NativeBindings.disunity_coroutine_start(WorldHandle, script, wait.Kind, wait.Num, wait.Den, &identity));
            routine.Identity = identity;
            Routines.Add(identity, routine);
            return routine;
        }
        catch { DisposeRoutine(routine); throw; }
    }

    internal static void StopCoroutine(MonoBehaviour owner, Coroutine routine)
    {
        if (!ReferenceEquals(owner, routine.Owner)) throw new ArgumentException("coroutine belongs to another script");
        if (routine.Completed) return;
        ThrowIfError(NativeBindings.disunity_coroutine_stop(WorldHandle, routine.Identity));
        DrainCancelled();
    }

    /// <summary>Stops each running coroutine of this script that was started from <paramref name="iterator"/>; none is not an error.</summary>
    internal static void StopCoroutine(MonoBehaviour owner, IEnumerator iterator)
    {
        ArgumentNullException.ThrowIfNull(iterator);
        foreach (var routine in Routines.Values.Where(r => ReferenceEquals(r.Owner, owner) && ReferenceEquals(r.Root, iterator)).ToArray())
            StopCoroutine(owner, routine);
    }

    internal static void StopAllCoroutines(MonoBehaviour owner)
    {
        foreach (var routine in Routines.Values.Where(r => ReferenceEquals(r.Owner, owner)).ToArray()) StopCoroutine(owner, routine);
    }

    /// <summary>Runs the coroutine to its next yield. A step that throws is logged against the
    /// owner and ends the coroutine, as in Unity.</summary>
    private static bool MoveIterator(Coroutine routine, out object? yielded)
    {
        yielded = null;
        while (routine.Iterators.TryPeek(out var iterator))
        {
            bool more;
            routine.Executing = true;
            try
            {
                more = iterator.MoveNext();
                if (more && !routine.Completed) yielded = iterator.Current;
            }
            catch (Exception error) when (IsGameException(error))
            {
                routine.Executing = false;
                ReportScriptException(error, routine.Owner);
                DisposeRoutine(routine);
                return false;
            }
            finally { routine.Executing = false; }
            if (routine.Completed) { DisposeRoutine(routine); return false; }
            if (!more)
            {
                routine.Iterators.Pop();
                DisposeIterator(routine, iterator);
                continue;
            }
            if (yielded is IEnumerator nested) { routine.Iterators.Push(nested); continue; }
            return true;
        }
        DisposeRoutine(routine);
        return false;
    }

    /// <summary>Disposes an iterator, running the coroutine's finally blocks. A block that
    /// throws is logged against the owner.</summary>
    private static void DisposeIterator(Coroutine routine, IEnumerator iterator)
    {
        try { (iterator as IDisposable)?.Dispose(); }
        catch (Exception error) when (IsGameException(error)) { ReportScriptException(error, routine.Owner); }
    }

    /// <summary>The native wait for what a coroutine yielded. An operation is polled each frame
    /// until it is done, then the coroutine resumes in the next frame.</summary>
    private static (uint Kind, ulong Num, ulong Den) DescribeWait(Coroutine routine, object? yielded)
    {
        routine.Waiting = yielded as AsyncOperation;
        return DescribeWait(yielded);
    }

    private static (uint Kind, ulong Num, ulong Den) DescribeWait(object? yielded) => yielded switch
    {
        null or AsyncOperation => (0, 0, 1),
        WaitForFixedUpdate => (1, 0, 1),
        WaitForEndOfFrame => (4, 0, 1),
        WaitForSeconds wait => (2, checked((ulong)Math.Round((double)wait.Seconds * 1_000_000_000)), 1_000_000_000),
        WaitForSecondsRealtime wait => (3, checked((ulong)Math.Round((double)wait.Seconds * 1_000_000_000)), 1_000_000_000),
        _ => throw new NotSupportedException($"unsupported coroutine yield {yielded.GetType().FullName}"),
    };

    private static void DispatchCoroutine(DisunityCallback payload)
    {
        var routine = Routines[payload.routine];
        try
        {
            object? yielded;
            if (routine.Waiting is { isDone: false } pending) yielded = pending;
            else if (!MoveIterator(routine, out yielded)) return;
            var wait = DescribeWait(routine, yielded);
            ThrowIfError(NativeBindings.disunity_callback_wait(WorldHandle, payload.ticket, wait.Kind, wait.Num, wait.Den));
        }
        catch { DisposeRoutine(routine); throw; }
    }

    internal static unsafe void DrainCancelled()
    {
        while (true)
        {
            ulong id;
            ThrowIfError(NativeBindings.disunity_coroutine_cancelled(WorldHandle, &id));
            if (id == 0) break;
            if (Routines.Remove(id, out var routine)) DisposeRoutine(routine);
        }
    }

    private static void DisposeAllRoutines()
    {
        foreach (var routine in Routines.Values.ToArray()) DisposeRoutine(routine);
    }

    private static void DisposeRoutine(Coroutine routine)
    {
        routine.Completed = true;
        Routines.Remove(routine.Identity);
        if (routine.Executing) return;
        while (routine.Iterators.TryPop(out var iterator)) DisposeIterator(routine, iterator);
    }
}
