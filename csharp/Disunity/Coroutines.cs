using System.Collections;
using Disunity.Hosting;

namespace UnityEngine;

public sealed class WaitForSeconds(float seconds) : YieldInstruction
{
    public float Seconds { get; } = float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForSecondsRealtime(float seconds)
{
    public float Seconds { get; } = float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForFixedUpdate : YieldInstruction;
public sealed class Coroutine : YieldInstruction
{
    internal readonly Stack<IEnumerator> Iterators = new();
    internal readonly MonoBehaviour Owner;
    internal ulong Identity;
    internal bool Completed;
    internal bool Executing;
    /// <summary>The unfinished operation the coroutine waits on, polled once a frame.</summary>
    internal AsyncOperation? Waiting;
    internal Coroutine(MonoBehaviour owner, IEnumerator iterator) { Owner = owner; Iterators.Push(iterator); }
}
