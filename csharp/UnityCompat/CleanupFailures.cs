using System.Runtime.ExceptionServices;

namespace CompatibilitySupport;

public sealed class CleanupFailures
{
    private readonly List<Exception> _errors = new();
    public void Add(Exception error) => _errors.Add(error);
    public void Run(Action cleanup)
    {
        try { cleanup(); }
        catch (Exception error) { Add(error); }
    }
    public void ThrowIfAny()
    {
        if (_errors.Count == 1) ExceptionDispatchInfo.Capture(_errors[0]).Throw();
        if (_errors.Count > 1) throw new AggregateException(_errors).Flatten();
    }
}
