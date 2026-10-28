namespace UnityEngine;

/// <summary>Evaluates a sorted key list: cubic Hermite segments, cubic Bezier segments where keys are
/// weighted, steps for infinite tangents, and the pre and post wrap modes outside the key range.</summary>
internal static class CurveEvaluation
{
    private const float DefaultWeight = 1f / 3f;

    public static float Evaluate(IReadOnlyList<Keyframe> keys, float time, WrapMode pre, WrapMode post)
    {
        if (keys.Count == 0) return 0f;
        if (keys.Count == 1) return keys[0].value;
        var first = keys[0].time;
        var last = keys[^1].time;
        if (time < first) time = Wrap(time, first, last, pre);
        else if (time > last) time = Wrap(time, first, last, post);
        return Segment(keys, time);
    }

    /// <summary>The time inside the key range that a time outside it reads. Loop repeats the range
    /// and PingPong mirrors every other repeat; every other mode holds the end key.</summary>
    private static float Wrap(float time, float first, float last, WrapMode mode)
    {
        var range = last - first;
        if (range <= 0f) return time < first ? first : last;
        switch (mode)
        {
            case WrapMode.Loop:
                return first + Repeat(time - first, range);
            case WrapMode.PingPong:
                var along = Repeat(time - first, range * 2f);
                return first + (along <= range ? along : range * 2f - along);
            default:
                return time < first ? first : last;
        }
    }

    private static float Repeat(float value, float length) => value - MathF.Floor(value / length) * length;

    private static float Segment(IReadOnlyList<Keyframe> keys, float time)
    {
        var index = 0;
        while (index < keys.Count - 2 && time >= keys[index + 1].time) index++;
        var (left, right) = (keys[index], keys[index + 1]);
        if (time >= right.time) return right.value;
        if (time <= left.time) return left.value;
        if (float.IsInfinity(left.outTangent) || float.IsInfinity(right.inTangent)) return left.value;
        var span = right.time - left.time;
        var outWeighted = ((int)left.weightedMode & (int)WeightedMode.Out) != 0;
        var inWeighted = ((int)right.weightedMode & (int)WeightedMode.In) != 0;
        if (!outWeighted && !inWeighted) return Hermite(left, right, (time - left.time) / span, span);
        return Bezier(left, right, time, outWeighted ? left.outWeight : DefaultWeight, inWeighted ? right.inWeight : DefaultWeight);
    }

    private static float Hermite(Keyframe left, Keyframe right, float s, float span)
    {
        var s2 = s * s;
        var s3 = s2 * s;
        return (2f * s3 - 3f * s2 + 1f) * left.value
            + (s3 - 2f * s2 + s) * span * left.outTangent
            + (-2f * s3 + 3f * s2) * right.value
            + (s3 - s2) * span * right.inTangent;
    }

    /// <summary>A cubic Bezier through both keys whose inner control points sit a weight's fraction of the
    /// segment's time span along each tangent. The parameter whose time is <paramref name="time"/> is found
    /// by bisection, since time is monotonic in the parameter for weights in [0, 1].</summary>
    private static float Bezier(Keyframe left, Keyframe right, float time, float outWeight, float inWeight)
    {
        var span = right.time - left.time;
        var x1 = left.time + outWeight * span;
        var x2 = right.time - inWeight * span;
        var y1 = left.value + left.outTangent * outWeight * span;
        var y2 = right.value - right.inTangent * inWeight * span;
        float low = 0f, high = 1f;
        for (var i = 0; i < 40; i++)
        {
            var mid = (low + high) * 0.5f;
            if (Cubic(left.time, x1, x2, right.time, mid) < time) low = mid;
            else high = mid;
        }
        return Cubic(left.value, y1, y2, right.value, (low + high) * 0.5f);
    }

    private static float Cubic(float p0, float p1, float p2, float p3, float u)
    {
        var v = 1f - u;
        return v * v * v * p0 + 3f * v * v * u * p1 + 3f * v * u * u * p2 + u * u * u * p3;
    }

    /// <summary>The slope of the line through the neighbors of a key, scaled by <c>1 - weight</c>. The first
    /// and last keys use their one neighbor, and a key without neighbors is flat.</summary>
    public static float SmoothSlope(IReadOnlyList<Keyframe> keys, int index, float weight)
    {
        var before = index > 0 ? keys[index - 1] : keys[index];
        var after = index < keys.Count - 1 ? keys[index + 1] : keys[index];
        var span = after.time - before.time;
        if (span <= 0f) return 0f;
        return (after.value - before.value) / span * (1f - Math.Clamp(weight, 0f, 1f));
    }
}
