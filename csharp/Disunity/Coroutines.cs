using System.Collections;
using Disunity.Hosting;

namespace UnityEngine;

public sealed class WaitForSeconds(float seconds)
{
    public float Seconds { get; } = float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForSecondsRealtime(float seconds)
{
    public float Seconds { get; } = float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForFixedUpdate;
public sealed class Coroutine
{
    internal readonly Stack<IEnumerator> Iterators = new();
    internal readonly MonoBehaviour Owner;
    internal ulong Identity;
    internal bool Completed;
    internal bool Executing;
    internal Coroutine(MonoBehaviour owner, IEnumerator iterator) { Owner = owner; Iterators.Push(iterator); }
}
