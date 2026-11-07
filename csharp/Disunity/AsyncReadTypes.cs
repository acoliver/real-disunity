namespace Unity.IO.LowLevel.Unsafe;

/// <summary>One read of a file: <see cref="Size"/> bytes from <see cref="Offset"/> into <see cref="Buffer"/>. Plain data; the
/// engine has no asynchronous read manager yet, so nothing runs a command.</summary>
public unsafe struct ReadCommand
{
    public void* Buffer;
    public long Offset;
    public long Size;
}

/// <summary>A list of reads: <see cref="CommandCount"/> commands starting at <see cref="ReadCommands"/>. Plain data.</summary>
public unsafe struct ReadCommandArray
{
    public ReadCommand* ReadCommands;
    public uint CommandCount;
}
