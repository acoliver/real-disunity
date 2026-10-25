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
            var wait = DescribeWait(yielded);
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

    internal static void StopAllCoroutines(MonoBehaviour owner)
    {
        foreach (var routine in Routines.Values.Where(r => ReferenceEquals(r.Owner, owner)).ToArray()) StopCoroutine(owner, routine);
    }

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
            finally { routine.Executing = false; }
            if (routine.Completed) { DisposeRoutine(routine); return false; }
            if (!more)
            {
                routine.Iterators.Pop();
                (iterator as IDisposable)?.Dispose();
                continue;
            }
            if (yielded is IEnumerator nested) { routine.Iterators.Push(nested); continue; }
            return true;
        }
        DisposeRoutine(routine);
        return false;
    }

    private static (uint Kind, ulong Num, ulong Den) DescribeWait(object? yielded) => yielded switch
    {
        null => (0, 0, 1),
        WaitForFixedUpdate => (1, 0, 1),
        WaitForSeconds wait => (2, checked((ulong)Math.Round((double)wait.Seconds * 1_000_000_000)), 1_000_000_000),
        WaitForSecondsRealtime wait => (3, checked((ulong)Math.Round((double)wait.Seconds * 1_000_000_000)), 1_000_000_000),
        _ => throw new NotSupportedException($"unsupported coroutine yield {yielded.GetType().FullName}"),
    };

    private static void DispatchCoroutine(DisunityCallback payload)
    {
        var routine = Routines[payload.routine];
        try
        {
            if (!MoveIterator(routine, out var yielded)) return;
            var wait = DescribeWait(yielded);
            ThrowIfError(NativeBindings.disunity_callback_wait(WorldHandle, payload.ticket, wait.Kind, wait.Num, wait.Den));
        }
        catch { DisposeRoutine(routine); throw; }
    }

    internal static unsafe void DrainCancelled()
    {
        List<Exception>? failures = null;
        while (true)
        {
            ulong id;
            ThrowIfError(NativeBindings.disunity_coroutine_cancelled(WorldHandle, &id));
            if (id == 0) break;
            if (Routines.Remove(id, out var routine))
            {
                try { DisposeRoutine(routine); }
                catch (Exception error) { (failures ??= new()).Add(error); }
            }
        }
        if (failures != null) throw new AggregateException(failures);
    }

    private static void DisposeAllRoutines()
    {
        List<Exception>? failures = null;
        foreach (var routine in Routines.Values.ToArray())
        {
            try { DisposeRoutine(routine); }
            catch (Exception error) { (failures ??= new()).Add(error); }
        }
        if (failures != null) throw new AggregateException(failures);
    }

    private static void DisposeRoutine(Coroutine routine)
    {
        routine.Completed = true;
        Routines.Remove(routine.Identity);
        if (routine.Executing) return;
        List<Exception>? failures = null;
        while (routine.Iterators.TryPop(out var iterator))
        {
            try { (iterator as IDisposable)?.Dispose(); }
            catch (Exception error) { (failures ??= new()).Add(error); }
        }
        if (failures != null) throw new AggregateException(failures);
    }
}
