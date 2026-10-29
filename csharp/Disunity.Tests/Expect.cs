/// <summary>Assertions shared by the tests of the native containers, jobs and attributes.</summary>
internal static class Expect
{
    public static void That(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    public static void Equal<T>(T expected, T actual, string what) =>
        That(EqualityComparer<T>.Default.Equals(expected, actual), $"{what}: expected {expected}, got {actual}");

    public static void Near(float expected, float actual, string what, float tolerance = 1e-4f) =>
        That(MathF.Abs(expected - actual) <= tolerance, $"{what}: expected {expected}, got {actual}");

    public static void Sequence<T>(IEnumerable<T> expected, IEnumerable<T> actual, string what) =>
        That(expected.SequenceEqual(actual), $"{what}: expected [{string.Join(", ", expected)}], got [{string.Join(", ", actual)}]");

    /// <summary>The action must throw exactly <typeparamref name="TException"/>; a subclass is a different contract.</summary>
    public static void Throws<TException>(Action action, string what) where TException : Exception
    {
        try { action(); }
        catch (Exception error)
        {
            That(error.GetType() == typeof(TException), $"{what}: expected {typeof(TException).Name}, got {error.GetType().Name}: {error.Message}");
            return;
        }
        throw new InvalidOperationException($"{what}: expected {typeof(TException).Name}, nothing was thrown");
    }

    /// <summary>Like <see cref="Throws{TException}"/>, and returns the exception so the caller can check its message.</summary>
    public static TException Catch<TException>(Action action, string what) where TException : Exception
    {
        try { action(); }
        catch (Exception error)
        {
            That(error.GetType() == typeof(TException), $"{what}: expected {typeof(TException).Name}, got {error.GetType().Name}: {error.Message}");
            return (TException)error;
        }
        throw new InvalidOperationException($"{what}: expected {typeof(TException).Name}, nothing was thrown");
    }
}
