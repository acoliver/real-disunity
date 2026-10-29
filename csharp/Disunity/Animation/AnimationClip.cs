using System.Collections.Concurrent;

namespace UnityEngine;

/// <summary>
/// A clip made by code. The engine loads no clip assets and plays none, so a clip holds its settings,
/// events and the curves set on it, and sampling it throws. Its length is the span of its curves' keys.
/// Curves set by code never animate the root or drive motion, so a clip has no root or motion curves.
/// </summary>
public sealed class AnimationClip : Motion
{
    private static readonly ConcurrentDictionary<int, WeakReference<AnimationClip>> Known = new();
    private readonly List<AnimationEvent> _events = new();
    private readonly AnimationCurves _curves = new();
    private bool _legacy;

    public AnimationClip() { }

    public float length => _curves.Length;

    public float frameRate { get; set; } = 60f;

    public WrapMode wrapMode { get; set; }
    public Bounds localBounds { get; set; }
    public new bool legacy { get => _legacy; set => _legacy = value; }
    public bool humanMotion => false;
    public bool empty => _curves.Empty;
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

    /// <summary>
    /// Sets the curve that animates <paramref name="propertyName"/> of the <paramref name="type"/>
    /// component at <paramref name="relativePath"/>, replacing any curve for it; a null curve removes
    /// it. SetCurve works at runtime only on legacy clips; on any other clip it logs an error and
    /// changes nothing.
    /// </summary>
    public void SetCurve(string relativePath, Type type, string propertyName, AnimationCurve curve)
    {
        ArgumentNullException.ThrowIfNull(relativePath);
        ArgumentNullException.ThrowIfNull(type);
        ArgumentNullException.ThrowIfNull(propertyName);
        if (!_legacy)
        {
            Debug.LogError("AnimationClip.SetCurve works at runtime only on legacy clips; set legacy to true first");
            return;
        }
        _curves.Set(relativePath, type, propertyName, curve);
    }

    public void ClearCurves() => _curves.Clear();

    internal AnimationCurve? FindCurve(string path, Type type, string property) => _curves.Find(path, type, property);

    public void EnsureQuaternionContinuity() => _curves.EnsureQuaternionContinuity();

    internal static void Remember(AnimationClip clip) =>
        Known[clip.GetInstanceID()] = new WeakReference<AnimationClip>(clip);

    internal static AnimationClip? FromInstanceID(int id) =>
        Known.TryGetValue(id, out var weak) && weak.TryGetTarget(out var clip) && !clip.Destroyed ? clip : null;

    internal override Object CloneAsset()
    {
        var copy = new AnimationClip { name = name + "(Clone)", frameRate = frameRate, _legacy = _legacy, wrapMode = wrapMode, localBounds = localBounds };
        foreach (var e in _events) copy._events.Add(e.Copy());
        _curves.CopyTo(copy._curves);
        return copy;
    }
}
