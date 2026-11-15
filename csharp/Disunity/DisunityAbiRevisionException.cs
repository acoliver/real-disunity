namespace Disunity.Hosting;

/// <summary>The loaded native library has a different ABI revision from this assembly.</summary>
public sealed class DisunityAbiRevisionException(uint expected, uint actual)
    : Exception($"ABI revision mismatch: expected {expected}, actual {actual}")
{
    public uint Expected { get; } = expected;
    public uint Actual { get; } = actual;
}
