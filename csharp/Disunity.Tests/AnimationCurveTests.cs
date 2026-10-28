using UnityEngine;

/// <summary>How <c>AnimationCurve.Evaluate</c> reads keys: Hermite segments from the tangents, Bezier
/// segments for weighted keys, steps for infinite tangents and the wrap modes. Expected values are
/// worked by hand from the cubic Hermite basis and the cubic Bezier form.</summary>
internal static class AnimationCurveTests
{
    public static void Run()
    {
        InterpolatesWithHermiteSegments();
        BuildsTheStandardCurves();
        InfiniteTangentsStep();
        WeightedKeysFollowTheirBezierSegments();
        WrapModesApplyOutsideTheKeys();
        SmallCurvesEvaluateSimply();
        Console.WriteLine("PASS: AnimationCurve.Evaluate follows Hermite, weighted and step segments and wrap modes");
    }

    private static AnimationCurve Two(Keyframe first, Keyframe second) => new(first, second);

    private static void InterpolatesWithHermiteSegments()
    {
        // With v0 = 0, m0 = 2, v1 = 1, m1 = 0 over [0, 1], p(s) = h10(s) * 2 + h01(s).
        var curve = Two(new Keyframe(0f, 0f, 0f, 2f), new Keyframe(1f, 1f, 0f, 0f));
        Expect.Near(0f, curve.Evaluate(0f), "the first key's value");
        Expect.Near(0.4375f, curve.Evaluate(0.25f), "Hermite at 0.25");
        Expect.Near(0.75f, curve.Evaluate(0.5f), "Hermite at 0.5");
        Expect.Near(1f, curve.Evaluate(1f), "the last key's value");
        // A segment [2, 4] scales the tangent terms by its span of 2: 5 + 0.25 + 7 - 0.75.
        var wide = Two(new Keyframe(2f, 10f, 0f, 1f), new Keyframe(4f, 14f, 3f, 0f));
        Expect.Near(11.5f, wide.Evaluate(3f), "a segment's tangents scale with its time span");
        var three = new AnimationCurve(new Keyframe(0f, 0f, 0f, 0f), new Keyframe(1f, 4f, 0f, 0f), new Keyframe(2f, 0f, 0f, 0f));
        Expect.Near(2f, three.Evaluate(0.5f), "the first segment of three keys");
        Expect.Near(2f, three.Evaluate(1.5f), "the second segment of three keys");
        Expect.Near(4f, three.Evaluate(1f), "a key in the middle");
    }

    private static void BuildsTheStandardCurves()
    {
        var ease = AnimationCurve.EaseInOut(0f, 0f, 1f, 1f);
        Expect.Near(0f, ease.Evaluate(0f), "EaseInOut starts at its start value");
        Expect.Near(1f, ease.Evaluate(1f), "EaseInOut ends at its end value");
        Expect.Near(0.5f, ease.Evaluate(0.5f), "EaseInOut passes the midpoint");
        Expect.Near(0.15625f, ease.Evaluate(0.25f), "EaseInOut is flat at its ends (3s^2 - 2s^3 at 0.25 is 0.15625)");
        Expect.Equal(0f, ease[0].outTangent, "EaseInOut's start tangent");
        var linear = AnimationCurve.Linear(0f, 0f, 2f, 4f);
        Expect.Equal(2f, linear[0].outTangent, "Linear's slope leaves the first key");
        Expect.Equal(2f, linear[1].inTangent, "Linear's slope arrives at the last key");
        Expect.Near(1f, linear.Evaluate(0.5f), "Linear at 0.5");
        Expect.Near(3f, linear.Evaluate(1.5f), "Linear at 1.5");
        var constant = AnimationCurve.Constant(1f, 3f, 7f);
        Expect.Equal(2, constant.length, "Constant has two keys");
        Expect.Near(7f, constant.Evaluate(2f), "Constant inside");
        Expect.Near(7f, constant.Evaluate(-10f), "Constant before");
        Expect.Near(7f, constant.Evaluate(10f), "Constant after");
    }

    private static void InfiniteTangentsStep()
    {
        var step = Two(new Keyframe(0f, 1f, 0f, float.PositiveInfinity), new Keyframe(1f, 5f, 0f, 0f));
        Expect.Equal(1f, step.Evaluate(0.01f), "an infinite out tangent holds the left value");
        Expect.Equal(1f, step.Evaluate(0.99f), "an infinite out tangent holds the left value until the next key");
        Expect.Equal(5f, step.Evaluate(1f), "the next key takes over at its time");
        var stepIn = Two(new Keyframe(0f, 1f, 0f, 0f), new Keyframe(1f, 5f, float.NegativeInfinity, 0f));
        Expect.Equal(1f, stepIn.Evaluate(0.5f), "an infinite in tangent on the right key steps too");
        var stairs = new AnimationCurve(
            new Keyframe(0f, 0f, 0f, float.PositiveInfinity),
            new Keyframe(1f, 2f, float.PositiveInfinity, float.PositiveInfinity),
            new Keyframe(2f, 3f, 0f, 0f));
        Expect.Equal(0f, stairs.Evaluate(0.9f), "the first step");
        Expect.Equal(2f, stairs.Evaluate(1.9f), "the second step");
    }

    private static void WeightedKeysFollowTheirBezierSegments()
    {
        // Weights of 1/3 on both sides are the Hermite curve: the Bezier's time is linear in its parameter.
        var third = 1f / 3f;
        var thirds = Two(new Keyframe(0f, 0f, 0f, 2f, third, third), new Keyframe(1f, 1f, 0f, 0f, third, third));
        Expect.Near(0.75f, thirds.Evaluate(0.5f), "weights of 1/3 equal the unweighted curve");
        // Out weight 0.5 on the left key: control points x = 0, 0.5, 2/3, 1 and y = 0, 1, 1, 1; u = 0.5 gives (0.5625, 0.875).
        var outWeighted = Two(new Keyframe(0f, 0f, 0f, 2f) { weightedMode = WeightedMode.Out, outWeight = 0.5f }, new Keyframe(1f, 1f, 0f, 0f));
        Expect.Near(0.875f, outWeighted.Evaluate(0.5625f), "an out weight bends the segment", 2e-4f);
        // In weight 0.5 on the right key with in tangent 2: x = 0, 1/3, 0.5, 1 and y = 0, 0, 0, 1; u = 0.5 gives (0.4375, 0.125).
        var inWeighted = Two(new Keyframe(0f, 0f, 0f, 0f), new Keyframe(1f, 1f, 2f, 0f) { weightedMode = WeightedMode.In, inWeight = 0.5f });
        Expect.Near(0.125f, inWeighted.Evaluate(0.4375f), "an in weight bends the segment", 2e-4f);
        // Weights are ignored when the mode does not name that side.
        var ignored = Two(new Keyframe(0f, 0f, 0f, 2f) { outWeight = 0.9f }, new Keyframe(1f, 1f, 0f, 0f));
        Expect.Near(0.75f, ignored.Evaluate(0.5f), "weights without a weighted mode are ignored");
        var wrongSide = Two(new Keyframe(0f, 0f, 0f, 2f) { weightedMode = WeightedMode.In, outWeight = 0.9f }, new Keyframe(1f, 1f, 0f, 0f));
        Expect.Near(0.75f, wrongSide.Evaluate(0.5f), "an out weight is ignored when only In is set");
    }

    private static void WrapModesApplyOutsideTheKeys()
    {
        foreach (var mode in new[] { WrapMode.ClampForever, WrapMode.Clamp, WrapMode.Once, WrapMode.Default })
        {
            var held = AnimationCurve.Linear(0f, 0f, 2f, 4f);
            held.preWrapMode = mode;
            held.postWrapMode = mode;
            Expect.Equal(0f, held.Evaluate(-3f), $"{mode} holds the first value before the keys");
            Expect.Equal(4f, held.Evaluate(5f), $"{mode} holds the last value after the keys");
        }
        var loop = AnimationCurve.Linear(0f, 0f, 2f, 4f);
        loop.preWrapMode = WrapMode.Loop;
        loop.postWrapMode = WrapMode.Loop;
        Expect.Near(2f, loop.Evaluate(3f), "Loop repeats after the keys");
        Expect.Near(3f, loop.Evaluate(-0.5f), "Loop repeats before the keys");
        Expect.Near(1f, loop.Evaluate(-1.5f), "Loop repeats before the keys by more than a period");
        Expect.Near(0f, loop.Evaluate(4f), "Loop returns to the first key at a whole period");
        var pingPong = AnimationCurve.Linear(0f, 0f, 2f, 4f);
        pingPong.preWrapMode = WrapMode.PingPong;
        pingPong.postWrapMode = WrapMode.PingPong;
        Expect.Near(2f, pingPong.Evaluate(3f), "PingPong mirrors after the keys");
        Expect.Near(3f, pingPong.Evaluate(2.5f), "PingPong mirrors just after the last key");
        Expect.Near(1f, pingPong.Evaluate(4.5f), "PingPong runs forward on every second pass");
        Expect.Near(1f, pingPong.Evaluate(-0.5f), "PingPong mirrors before the keys");
        var mixed = AnimationCurve.Linear(0f, 0f, 2f, 4f);
        mixed.preWrapMode = WrapMode.Loop;
        Expect.Near(3f, mixed.Evaluate(-0.5f), "the pre mode applies before the keys");
        Expect.Near(4f, mixed.Evaluate(3f), "the post mode applies after the keys");
    }

    private static void SmallCurvesEvaluateSimply()
    {
        Expect.Equal(0f, new AnimationCurve().Evaluate(3f), "an empty curve is 0");
        var single = new AnimationCurve(new Keyframe(2f, 9f));
        Expect.Equal(9f, single.Evaluate(-5f), "one key before");
        Expect.Equal(9f, single.Evaluate(2f), "one key at");
        Expect.Equal(9f, single.Evaluate(8f), "one key after");
        single.postWrapMode = WrapMode.Loop;
        Expect.Equal(9f, single.Evaluate(8f), "one key ignores wrapping");
    }
}
