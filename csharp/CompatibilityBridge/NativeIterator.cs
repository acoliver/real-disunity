using UnityEngine;

namespace CompatibilityBridge;

internal sealed partial class NativeReadiness
{
    private bool ExecuteRoutine(ulong ticket, ulong id, string kind)
    {
        var routine = _routines[id];
        object? wait = null;
        var outcome = "failed";
        try
        {
            switch (kind)
            {
                case "MoveNext":
                    routine.Join = null;
                    outcome = routine.Iterator!.MoveNext() ? "more" : "done";
                    break;
                case "Current":
                    var current = routine.Iterator!.Current;
                    wait = NativeWait.Describe(current, Find);
                    routine.Join = current is Coroutine join ? Find(join) : null;
                    outcome = "ok";
                    break;
                case "Dispose":
                    routine.Join = null;
                    (routine.Iterator as IDisposable)?.Dispose();
                    outcome = "ok";
                    break;
                default: throw new InvalidDataException("unknown native iterator decision");
            }
        }
        catch (Exception error) { failed(routine.Coroutine.Owner, kind, error); }
        finally
        {
            Complete(ticket, outcome);
            if (kind == "Dispose")
            {
                routine.Iterator = null;
                routine.Coroutine.Completed = true;
            }
        }
        if (kind != "Current" || outcome != "ok") return kind == "Dispose";
        try { Request("suspend", ("routine", id), ("wait", wait)); return true; }
        catch (NativeWorldException error)
        {
            routine.Join = null;
            // v1 preserves a cancellation latched during Current instead of entering Suspend.
            if (error.Code != "invalid_request" || error.Message != "[invalid_request] suspend requires a completed Current claim")
                failed(routine.Coroutine.Owner, "Current", error);
            Request("cancel", ("routine", id));
            return false;
        }
    }
}
