using UnityEngine.Rendering;

/// <summary>StencilState defaults, face-specific values and the setters that set both faces.</summary>
internal static class StencilStateTests
{
    public static void Run()
    {
        var state = new StencilState();
        Expect.That(!state.enabled && state.readMask == 255 && state.writeMask == 255, "the default test is disabled with full masks");
        Expect.That(state.compareFunctionFront == CompareFunction.Always && state.compareFunctionBack == CompareFunction.Always, "the default comparison is Always");
        Expect.That(state.passOperationFront == StencilOp.Keep && state.failOperationBack == StencilOp.Keep && state.zFailOperationFront == StencilOp.Keep, "the default operations keep");
        Expect.That(StencilState.defaultValue == state, "defaultValue is the default");

        state.enabled = true;
        state.readMask = 3;
        state.writeMask = 0;
        state.compareFunctionFront = CompareFunction.Less;
        state.passOperationBack = StencilOp.Replace;
        Expect.That(state.enabled && state.readMask == 3 && state.writeMask == 0, "flags and masks are stored");
        Expect.That(state.compareFunctionFront == CompareFunction.Less && state.compareFunctionBack == CompareFunction.Always, "a comparison is kept per face");
        Expect.That(state.passOperationBack == StencilOp.Replace && state.passOperationFront == StencilOp.Keep, "an operation is kept per face");

        state.SetCompareFunction(CompareFunction.Never);
        state.SetPassOperation(StencilOp.Invert);
        state.SetFailOperation(StencilOp.Zero);
        state.SetZFailOperation(StencilOp.IncrementWrap);
        Expect.That(state.compareFunctionFront == CompareFunction.Never && state.compareFunctionBack == CompareFunction.Never, "SetCompareFunction sets both faces");
        Expect.That(state.passOperationFront == StencilOp.Invert && state.passOperationBack == StencilOp.Invert, "SetPassOperation sets both faces");
        Expect.That(state.failOperationFront == StencilOp.Zero && state.failOperationBack == StencilOp.Zero, "SetFailOperation sets both faces");
        Expect.That(state.zFailOperationFront == StencilOp.IncrementWrap && state.zFailOperationBack == StencilOp.IncrementWrap, "SetZFailOperation sets both faces");

        var copy = state;
        Expect.That(copy == state && copy.GetHashCode() == state.GetHashCode() && copy.Equals((object)state), "equal states compare and hash equal");
        copy.writeMask = 9;
        Expect.That(copy != state, "states differing in a mask are unequal");
        RenderQueueRanges();
        Console.WriteLine("PASS: StencilState defaults, face values and both-face setters, and RenderQueueRange values");
    }

    private static void RenderQueueRanges()
    {
        Expect.That(RenderQueueRange.all is { lowerBound: 0, upperBound: 5000 }, "all queues");
        Expect.That(RenderQueueRange.opaque is { lowerBound: 0, upperBound: 2500 }, "opaque queues");
        Expect.That(RenderQueueRange.transparent is { lowerBound: 2501, upperBound: 5000 }, "transparent queues start above the opaque ones");
        Expect.That(RenderQueueRange.opaque == new RenderQueueRange { lowerBound = 0, upperBound = 2500 } && RenderQueueRange.opaque != RenderQueueRange.transparent, "ranges compare by bounds");
        Expect.That(RenderQueueRange.all.GetHashCode() == RenderQueueRange.all.GetHashCode() && RenderQueueRange.all.Equals((object)RenderQueueRange.all), "equal ranges hash and compare as objects");
    }
}
