namespace UnityEngine;

/// <summary>
/// The curves a clip made by code holds, keyed by object path, component type and property, in the
/// order they were set. The clip keeps its own copies, so changing a curve after setting it does not
/// change the clip.
/// </summary>
internal sealed class AnimationCurves
{
    private readonly List<(string Path, Type Type, string Property, AnimationCurve Curve)> _curves = new();

    public bool Empty => _curves.Count == 0;

    /// <summary>The span from the earliest key to the latest across the curves, 0 with no keys.</summary>
    public float Length
    {
        get
        {
            var times = _curves.SelectMany(entry => entry.Curve.keys).Select(key => key.time).ToList();
            return times.Count == 0 ? 0f : times.Max() - times.Min();
        }
    }

    public void Set(string path, Type type, string property, AnimationCurve? curve)
    {
        var at = _curves.FindIndex(entry => entry.Path == path && entry.Type == type && entry.Property == property);
        if (curve is null)
        {
            if (at >= 0) _curves.RemoveAt(at);
            return;
        }
        var entry = (path, type, property, Copy(curve));
        if (at >= 0) _curves[at] = entry;
        else _curves.Add(entry);
    }

    public void Clear() => _curves.Clear();

    /// <summary>The clip's curve for a property, for tests; Unity reads clip curves only in the editor.</summary>
    public AnimationCurve? Find(string path, Type type, string property) =>
        _curves.FirstOrDefault(entry => entry.Path == path && entry.Type == type && entry.Property == property).Curve;

    public void CopyTo(AnimationCurves other)
    {
        foreach (var (path, type, property, curve) in _curves) other._curves.Add((path, type, property, Copy(curve)));
    }

    /// <summary>
    /// Negates whole rotation keys where needed so that each key's quaternion is in the same
    /// hemisphere as the previous key's, which makes the interpolation take the short path. A
    /// rotation is the four curves <c>&lt;name&gt;.x</c> to <c>.w</c> of one path and type with the
    /// same key times; rotations whose curves have different key times are not supported.
    /// </summary>
    public void EnsureQuaternionContinuity()
    {
        foreach (var group in _curves.Where(entry => IsRotationPart(entry.Property))
                     .GroupBy(entry => (entry.Path, entry.Type, Name: entry.Property[..^2])))
        {
            var parts = "xyzw".Select(axis => group.FirstOrDefault(entry => entry.Property[^1] == axis).Curve).ToArray();
            if (parts.Any(part => part is null)) continue;
            var keys = parts.Select(part => part!.keys).ToArray();
            if (keys.Any(k => k.Length != keys[0].Length || !k.Select(key => key.time).SequenceEqual(keys[0].Select(key => key.time))))
                throw new NotSupportedException($"AnimationClip.EnsureQuaternionContinuity is not supported yet for rotation curves of {group.Key.Name} whose keys are at different times");
            for (var i = 1; i < keys[0].Length; i++)
            {
                var dot = 0f;
                for (var c = 0; c < 4; c++) dot += keys[c][i - 1].value * keys[c][i].value;
                if (dot < 0f) for (var c = 0; c < 4; c++) keys[c][i] = Negated(keys[c][i]);
            }
            for (var c = 0; c < 4; c++) parts[c]!.keys = keys[c];
        }
    }

    private static bool IsRotationPart(string property) =>
        property.Length > 2 && property[^2] == '.' && "xyzw".Contains(property[^1]);

    private static Keyframe Negated(Keyframe key)
    {
        key.value = -key.value;
        key.inTangent = -key.inTangent;
        key.outTangent = -key.outTangent;
        return key;
    }

    private static AnimationCurve Copy(AnimationCurve curve)
    {
        var copy = new AnimationCurve();
        copy.CopyFrom(curve);
        return copy;
    }
}
