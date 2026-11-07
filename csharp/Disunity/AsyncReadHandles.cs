using Unity.Jobs;

namespace Unity.IO.LowLevel.Unsafe;

/// <summary>
/// A file opened for asynchronous reads. The engine has no asynchronous read manager, so no handle is ever valid: a default
/// handle is the only value, <see cref="IsValid"/> is false, <see cref="Status"/> is <see cref="FileState.Absent"/> and
/// <see cref="Close"/> returns a completed job.
/// </summary>
public struct FileHandle
{
    public readonly JobHandle JobHandle => default;
    public readonly FileState Status => FileState.Absent;
    public readonly bool IsValid() => false;
    public readonly JobHandle Close() => default;
}

/// <summary>
/// A read in flight. The engine has no asynchronous read manager, so no handle is ever valid: a default handle is the only
/// value, <see cref="IsValid"/> is false, <see cref="Status"/> is <see cref="ReadStatus.Failed"/> and <see cref="Dispose"/>
/// does nothing.
/// </summary>
public struct ReadHandle : IDisposable
{
    public readonly JobHandle JobHandle => default;
    public readonly ReadStatus Status => ReadStatus.Failed;
    public readonly bool IsValid() => false;
    public readonly long GetBytesRead() => 0;
    public readonly void Cancel() { }
    public readonly void Dispose() { }
}
