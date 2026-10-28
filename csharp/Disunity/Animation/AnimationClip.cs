using System.Collections.Concurrent;

namespace UnityEngine;

/// <summary>
/// A clip made by code. The engine loads no clip assets and plays none, so a clip holds its settings and
/// events and has no curves: <c>SetCurve</c> needs AnimationCurve, which is not defined yet, and a clip
/// with no curves is empty, has zero length and no root or motion curves. Sampling it throws.
/// </summary>
public sealed class AnimationClip : Motion
{
    private static readonly ConcurrentDictionary<int, WeakReference<AnimationClip>> Known = new();
    private readonly List<AnimationEvent> _events = new();
    private bool _legacy;

    public AnimationClip() { }

    public float length => 0f;

    public float frameRate { get; set; } = 60f;

    public WrapMode wrapMode { get; set; }
    public Bounds localBounds { get; set; }
    public new bool legacy { get => _legacy; set => _legacy = value; }
    public bool humanMotion => false;
    public bool empty => true;
    public bool hasGenericRootTransform => false;
    public bool hasMotionFloatCurves => false;
    public bool hasMotionCurves => false;
    public bool hasRootCurves => false;

    public override float averageDuration => length;
    public override bool isLooping => wrapMode is WrapMode.Loop or WrapMode.PingPong;
    public override bool isHumanMotion => humanMotion;
    internal override bool IsLegacy => _legacy;

    /// <summary>The clip's events in time order; a copy, so changing it changes nothing until assigned back.</summary>
    public AnimationEvent[] events
    {
        get => _events.Select(e => e.Copy()).ToArray();
        set
        {
            ArgumentNullException.ThrowIfNull(value);
            _events.Clear();
            foreach (var e in value) AddEvent(e);
        }
    }

    /// <summary>Stores a copy of the event after the last one that is not later, so equal times keep their order.</summary>
    public void AddEvent(AnimationEvent evt)
    {
        ArgumentNullException.ThrowIfNull(evt);
        var at = _events.FindLastIndex(e => e.time <= evt.time) + 1;
        _events.Insert(at, evt.Copy());
    }

    public void SampleAnimation(GameObject go, float time) =>
        throw new NotSupportedException("AnimationClip.SampleAnimation is not supported yet: the engine plays no animation");

    /// <summary>Removes the clip's curves, of which it has none.</summary>
    public void ClearCurves() { }

    /// <summary>Makes quaternion curves continuous; a clip with no curves is already.</summary>
    public void EnsureQuaternionContinuity() { }

    internal static void Remember(AnimationClip clip) =>
        Known[clip.GetInstanceID()] = new WeakReference<AnimationClip>(clip);

    internal static AnimationClip? FromInstanceID(int id) =>
        Known.TryGetValue(id, out var weak) && weak.TryGetTarget(out var clip) && !clip.Destroyed ? clip : null;

    internal override Object CloneAsset()
    {
        var copy = new AnimationClip { name = name + "(Clone)", frameRate = frameRate, _legacy = _legacy, wrapMode = wrapMode, localBounds = localBounds };
        foreach (var e in _events) copy._events.Add(e.Copy());
        return copy;
    }
}
