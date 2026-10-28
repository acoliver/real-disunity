using UnityEngine;

/// <summary>How an <c>AnimationCurve</c> edits its keys: sorted insertion, duplicate times, moving,
/// removing, smoothing, copying and equality, plus the <c>Keyframe</c> constructors.</summary>
internal static class CurveEditingTests
{
    public static void Run()
    {
        KeyframeConstructorsSetTheirFields();
        AddKeyKeepsKeysSorted();
        AddKeyByTimeAndValueSmoothsTheCurve();
        MoveAndRemoveKeepTheOrder();
        SmoothTangentsUsesTheNeighbours();
        KeysAreCopiedInAndOut();
        CopyAndEquality();
        BadIndicesThrow();
        Console.WriteLine("PASS: AnimationCurve edits, copies and compares its keys like Unity's");
    }

    private static IEnumerable<float> Times(AnimationCurve curve) => curve.keys.Select(key => key.time);

    private static void KeyframeConstructorsSetTheirFields()
    {
        var plain = new Keyframe(1f, 2f);
        Expect.That(plain.time == 1f && plain.value == 2f && plain.inTangent == 0f && plain.outTangent == 0f, "two arguments are a flat key");
        Expect.Equal(WeightedMode.None, plain.weightedMode, "a plain key is not weighted");
        var tangents = new Keyframe(1f, 2f, 3f, 4f);
        Expect.That(tangents.inTangent == 3f && tangents.outTangent == 4f, "tangents");
        Expect.Equal(WeightedMode.None, tangents.weightedMode, "tangents alone do not weight the key");
        var weighted = new Keyframe(1f, 2f, 3f, 4f, 0.25f, 0.75f);
        Expect.That(weighted.inWeight == 0.25f && weighted.outWeight == 0.75f, "weights");
        Expect.Equal(WeightedMode.Both, weighted.weightedMode, "weights weight both sides");
        weighted.weightedMode = WeightedMode.Out;
        weighted.time = 5f;
        Expect.That(weighted.weightedMode == WeightedMode.Out && weighted.time == 5f, "the properties set");
    }

    private static void AddKeyKeepsKeysSorted()
    {
        var curve = new AnimationCurve();
        Expect.Equal(0, curve.AddKey(new Keyframe(1f, 10f)), "the first key");
        Expect.Equal(1, curve.AddKey(new Keyframe(3f, 30f)), "a later key goes last");
        Expect.Equal(1, curve.AddKey(new Keyframe(2f, 20f)), "a key between goes in the middle");
        Expect.Equal(0, curve.AddKey(new Keyframe(0f, 0f)), "an earlier key goes first");
        Expect.Sequence(new[] { 0f, 1f, 2f, 3f }, Times(curve), "keys are sorted by time");
        Expect.Equal(-1, curve.AddKey(new Keyframe(2f, 99f)), "a second key at the same time is refused");
        Expect.Equal(-1, curve.AddKey(2f, 99f), "AddKey by time and value refuses a duplicate too");
        Expect.Equal(4, curve.length, "a refused key is not added");
        Expect.Equal(20f, curve[2].value, "the existing key is unchanged");
        var tangents = new AnimationCurve();
        tangents.AddKey(new Keyframe(0f, 0f, 7f, 8f));
        Expect.That(tangents[0].inTangent == 7f && tangents[0].outTangent == 8f, "AddKey(Keyframe) keeps the key's tangents");
    }

    private static void AddKeyByTimeAndValueSmoothsTheCurve()
    {
        var curve = new AnimationCurve();
        Expect.Equal(0, curve.AddKey(0f, 0f), "first");
        Expect.Equal(1, curve.AddKey(1f, 1f), "second");
        Expect.Near(1f, curve[0].outTangent, "two added keys are joined by their slope");
        Expect.Near(0.5f, curve.Evaluate(0.5f), "so the curve is a line");
        curve.AddKey(2f, 4f);
        Expect.Near(2f, curve[1].inTangent, "the middle key takes the slope through its neighbours, (4 - 0) / 2");
    }

    private static void MoveAndRemoveKeepTheOrder()
    {
        var curve = new AnimationCurve(new Keyframe(0f, 0f), new Keyframe(1f, 1f), new Keyframe(2f, 2f));
        Expect.Equal(2, curve.MoveKey(0, new Keyframe(3f, 9f)), "a key moved past the others gets the last index");
        Expect.Sequence(new[] { 1f, 2f, 3f }, Times(curve), "moved keys stay sorted");
        Expect.Equal(9f, curve[2].value, "the moved key carries the new value");
        Expect.Equal(-1, curve.MoveKey(1, new Keyframe(1f, 5f)), "moving onto another key's time is refused");
        Expect.Sequence(new[] { 1f, 2f, 3f }, Times(curve), "a refused move changes nothing");
        Expect.Equal(2f, curve[1].value, "the refused move kept the key's value");
        curve.RemoveKey(0);
        Expect.Sequence(new[] { 2f, 3f }, Times(curve), "RemoveKey removes the indexed key");
        curve.ClearKeys();
        Expect.Equal(0, curve.length, "ClearKeys empties the curve");
        Expect.Equal(0f, curve.Evaluate(1f), "an emptied curve evaluates to 0");
    }

    private static void SmoothTangentsUsesTheNeighbours()
    {
        var curve = new AnimationCurve(new Keyframe(0f, 0f), new Keyframe(1f, 1f), new Keyframe(2f, 4f));
        curve.SmoothTangents(1, 0f);
        Expect.That(curve[1].inTangent == 2f && curve[1].outTangent == 2f, "slope (4 - 0) / 2 on both sides");
        curve.SmoothTangents(1, 0.5f);
        Expect.That(curve[1].inTangent == 1f && curve[1].outTangent == 1f, "weight 0.5 halves the smoothing");
        curve.SmoothTangents(1, 1f);
        Expect.That(curve[1].inTangent == 0f, "weight 1 leaves the key flat");
        curve.SmoothTangents(0, 0f);
        Expect.That(curve[0].outTangent == 1f, "the first key uses its one neighbour");
        curve.SmoothTangents(2, 0f);
        Expect.That(curve[2].inTangent == 3f, "the last key uses its one neighbour");
        var single = new AnimationCurve(new Keyframe(0f, 5f, 1f, 1f));
        single.SmoothTangents(0, 0f);
        Expect.That(single[0].outTangent == 0f, "a lone key is flat");
    }

    private static void KeysAreCopiedInAndOut()
    {
        var curve = new AnimationCurve(new Keyframe(2f, 2f), new Keyframe(0f, 0f));
        Expect.Sequence(new[] { 0f, 2f }, Times(curve), "keys given out of order are sorted");
        var copy = curve.keys;
        copy[0].value = 99f;
        Expect.Equal(0f, curve[0].value, "the keys array is a copy");
        var source = new[] { new Keyframe(0f, 1f), new Keyframe(1f, 2f) };
        curve.keys = source;
        source[0].value = 50f;
        Expect.Equal(1f, curve[0].value, "setting keys copies them in");
        Expect.Equal(2, curve.length, "length");
        var span = new Keyframe[3];
        curve.GetKeys(span);
        Expect.That(span[1].value == 2f, "GetKeys fills the span");
        Expect.Throws<ArgumentException>(() => curve.GetKeys(new Keyframe[1]), "a short span");
        curve.SetKeys([new Keyframe(5f, 6f)]);
        Expect.That(curve.length == 1 && curve[0].time == 5f, "SetKeys replaces the keys");
        Expect.Equal(0, new AnimationCurve(null!).length, "a null key array is an empty curve");
    }

    private static void CopyAndEquality()
    {
        var source = AnimationCurve.Linear(0f, 0f, 1f, 1f);
        source.preWrapMode = WrapMode.Loop;
        source.postWrapMode = WrapMode.PingPong;
        var copy = new AnimationCurve();
        copy.CopyFrom(source);
        Expect.That(copy.Equals(source) && copy.Equals((object)source), "CopyFrom makes an equal curve");
        Expect.Equal(source.GetHashCode(), copy.GetHashCode(), "equal curves hash equally");
        Expect.Equal(WrapMode.Loop, copy.preWrapMode, "CopyFrom copies the pre wrap mode");
        Expect.Equal(WrapMode.PingPong, copy.postWrapMode, "CopyFrom copies the post wrap mode");
        copy.RemoveKey(1);
        Expect.That(source.length == 2, "the copy is independent");
        Expect.That(!copy.Equals(source), "curves with different keys differ");
        var other = AnimationCurve.Linear(0f, 0f, 1f, 1f);
        Expect.That(!other.Equals(source), "wrap modes take part in equality");
        Expect.That(!source.Equals((AnimationCurve?)null) && !source.Equals((object?)null) && !source.Equals("x"), "no curve equals null or another type");
        Expect.Throws<ArgumentNullException>(() => copy.CopyFrom(null!), "CopyFrom null");
    }

    private static void BadIndicesThrow()
    {
        var curve = new AnimationCurve(new Keyframe(0f, 0f));
        Expect.Throws<IndexOutOfRangeException>(() => _ = curve[1], "the indexer");
        Expect.Throws<IndexOutOfRangeException>(() => _ = curve[-1], "a negative index");
        Expect.Throws<IndexOutOfRangeException>(() => curve.RemoveKey(1), "RemoveKey");
        Expect.Throws<IndexOutOfRangeException>(() => curve.MoveKey(2, new Keyframe(1f, 1f)), "MoveKey");
        Expect.Throws<IndexOutOfRangeException>(() => curve.SmoothTangents(5, 0f), "SmoothTangents");
    }
}
