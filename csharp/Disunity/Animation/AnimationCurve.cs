namespace UnityEngine;

/// <summary>A list of <see cref="Keyframe"/> values sorted by time that interpolates between them.
/// A new curve clamps before its first key and after its last, as Unity's does. Equality compares
/// keys and wrap modes.</summary>
public class AnimationCurve : IEquatable<AnimationCurve>
{
    /// <summary>Two keys closer in time than this are one key, as in Unity.</summary>
    private const float TimeEpsilon = 0.00001f;

    private readonly List<Keyframe> keyList = [];

    public AnimationCurve(params Keyframe[] keys)
    {
        if (keys != null) this.keys = keys;
        preWrapMode = WrapMode.ClampForever;
        postWrapMode = WrapMode.ClampForever;
    }

    public AnimationCurve() : this(Array.Empty<Keyframe>()) { }

    /// <summary>A copy of the keys. Setting copies the keys in and sorts them by time.</summary>
    public Keyframe[] keys
    {
        get => keyList.ToArray();
        set
        {
            ArgumentNullException.ThrowIfNull(value);
            keyList.Clear();
            keyList.AddRange(value.OrderBy(key => key.time));
        }
    }

    public int length => keyList.Count;

    public WrapMode preWrapMode { get; set; }

    public WrapMode postWrapMode { get; set; }

    public Keyframe this[int index] => keyList[Checked(index)];

    public float Evaluate(float time) => CurveEvaluation.Evaluate(keyList, time, preWrapMode, postWrapMode);

    /// <summary>Adds a key and smooths its tangents and its neighbors' with <see cref="SmoothTangents"/>
    /// at weight 0, so keys added by time and value give a smooth curve. Returns the new index, or -1
    /// when a key already exists at that time.</summary>
    public int AddKey(float time, float value)
    {
        var index = AddKey(new Keyframe(time, value));
        if (index < 0) return index;
        for (var near = Math.Max(0, index - 1); near <= Math.Min(keyList.Count - 1, index + 1); near++)
            SmoothTangents(near, 0f);
        return index;
    }

    /// <summary>Adds a key with its own tangents. Returns its index, or -1 when a key already exists at that time.</summary>
    public int AddKey(Keyframe key)
    {
        var index = 0;
        while (index < keyList.Count && keyList[index].time < key.time - TimeEpsilon) index++;
        if (index < keyList.Count && Math.Abs(keyList[index].time - key.time) <= TimeEpsilon) return -1;
        keyList.Insert(index, key);
        return index;
    }

    /// <summary>Replaces the key at <paramref name="index"/> and keeps the keys sorted. Returns the key's
    /// new index, or -1 and changes nothing when another key already sits at the new time.</summary>
    public int MoveKey(int index, Keyframe key)
    {
        Checked(index);
        var kept = keyList[index];
        keyList.RemoveAt(index);
        var moved = AddKey(key);
        if (moved >= 0) return moved;
        AddKey(kept);
        return -1;
    }

    public void RemoveKey(int index) => keyList.RemoveAt(Checked(index));

    public void ClearKeys() => keyList.Clear();

    public void GetKeys(Span<Keyframe> keys)
    {
        if (keys.Length < keyList.Count) throw new ArgumentException("The span is shorter than the number of keys.", nameof(keys));
        for (var i = 0; i < keyList.Count; i++) keys[i] = keyList[i];
    }

    public void SetKeys(ReadOnlySpan<Keyframe> keys) => this.keys = keys.ToArray();

    /// <summary>Sets the key's tangents to the slope through its neighbors, scaled by <c>1 - weight</c>;
    /// weight 0 is the full smoothing and weight 1 leaves the key flat.</summary>
    public void SmoothTangents(int index, float weight)
    {
        var slope = CurveEvaluation.SmoothSlope(keyList, Checked(index), weight);
        var key = keyList[index];
        key.inTangent = slope;
        key.outTangent = slope;
        keyList[index] = key;
    }

    public static AnimationCurve Constant(float timeStart, float timeEnd, float value) =>
        new(new Keyframe(timeStart, value, 0f, 0f), new Keyframe(timeEnd, value, 0f, 0f));

    public static AnimationCurve Linear(float timeStart, float valueStart, float timeEnd, float valueEnd)
    {
        var slope = (valueEnd - valueStart) / (timeEnd - timeStart);
        return new(new Keyframe(timeStart, valueStart, 0f, slope), new Keyframe(timeEnd, valueEnd, slope, 0f));
    }

    public static AnimationCurve EaseInOut(float timeStart, float valueStart, float timeEnd, float valueEnd) =>
        new(new Keyframe(timeStart, valueStart, 0f, 0f), new Keyframe(timeEnd, valueEnd, 0f, 0f));

    /// <summary>Copies the other curve's keys and wrap modes into this one.</summary>
    public void CopyFrom(AnimationCurve other)
    {
        ArgumentNullException.ThrowIfNull(other);
        if (ReferenceEquals(other, this)) return;
        keyList.Clear();
        keyList.AddRange(other.keyList);
        preWrapMode = other.preWrapMode;
        postWrapMode = other.postWrapMode;
    }

    public override bool Equals(object? o) => o is AnimationCurve other && Equals(other);

    public bool Equals(AnimationCurve? other) =>
        other is not null
        && (ReferenceEquals(this, other)
            || (preWrapMode == other.preWrapMode && postWrapMode == other.postWrapMode
                && keyList.Count == other.keyList.Count && keyList.Zip(other.keyList).All(pair => pair.First.SameAs(pair.Second))));

    public override int GetHashCode()
    {
        var hash = new HashCode();
        hash.Add(preWrapMode);
        hash.Add(postWrapMode);
        foreach (var key in keyList) hash.Add(key.Hash());
        return hash.ToHashCode();
    }

    private int Checked(int index) =>
        index >= 0 && index < keyList.Count ? index : throw new IndexOutOfRangeException("Index out of bounds.");
}
