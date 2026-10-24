using System.Collections;

namespace CompatibilityBridge;

internal sealed class StartRoutine(IEnumerator routine, Action<Exception> failed) : IEnumerator, IDisposable
{
    public object? Current { get; private set; }
    public bool MoveNext()
    {
        try
        {
            if (!routine.MoveNext()) return false;
            Current = routine.Current;
            return true;
        }
        catch (Exception error)
        {
            failed(error);
            return false;
        }
    }
    public void Reset() => routine.Reset();
    public void Dispose() => (routine as IDisposable)?.Dispose();
}
