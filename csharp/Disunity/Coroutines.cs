using System.Collections;
using Disunity.Hosting;

namespace UnityEngine;

public sealed class WaitForSeconds(float seconds) : YieldInstruction
{
    public float Seconds { get; } = float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForSecondsRealtime(float time)
{
    private float _waitTime = Checked(time);

    /// <summary>The wait in unscaled seconds. A coroutine reads it when it yields this object, so changing it afterwards does not
    /// change a wait already under way.</summary>
    public float waitTime
    {
        get => _waitTime;
        set => _waitTime = Checked(value);
    }

    public float Seconds => _waitTime;

    private static float Checked(float seconds) => float.IsFinite(seconds) && seconds >= 0 ? seconds : throw new ArgumentOutOfRangeException(nameof(seconds));
}
public sealed class WaitForFixedUpdate : YieldInstruction;

/// <summary>Resumes the coroutine once, at the end of the frame, after every LateUpdate. A coroutine that yields it
/// from that point resumes at the end of the next frame.</summary>
public sealed class WaitForEndOfFrame : YieldInstruction;
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
