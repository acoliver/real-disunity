using Disunity.Hosting;
using UnityEngine;

/// <summary>Instantiate copies serialized AnimationCurve and Gradient fields: each clone has its own object with the same
/// keys and settings.</summary>
internal static class CurveCloneTests
{
    private sealed class Shaped : MonoBehaviour
    {
        public AnimationCurve fade = new(new Keyframe(0, 0, 0, 1), new Keyframe(1, 2, 1, 0))
        {
            preWrapMode = WrapMode.Loop, postWrapMode = WrapMode.PingPong,
        };
        public Gradient tint = new() { mode = GradientMode.Fixed, colorSpace = ColorSpace.Linear };
        public AnimationCurve[] steps = [new(new Keyframe(0, 5))];
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var owner = new GameObject("shaped");
            var original = owner.AddComponent<Shaped>();
            original.tint.SetKeys([new GradientColorKey(Color.red, 0), new GradientColorKey(Color.blue, 1)], [new GradientAlphaKey(0.5f, 0.25f)]);
            var clone = UnityEngine.Object.Instantiate(owner).GetComponent<Shaped>()!;
            Expect.That(!ReferenceEquals(clone.fade, original.fade), "the clone has its own curve");
            Expect.Sequence(original.fade.keys, clone.fade.keys, "the curve keys are copied");
            Expect.Equal(WrapMode.Loop, clone.fade.preWrapMode, "the pre-wrap mode is copied");
            Expect.Equal(WrapMode.PingPong, clone.fade.postWrapMode, "the post-wrap mode is copied");
            clone.fade.AddKey(0.5f, 9);
            Expect.Equal(2, original.fade.length, "changing the clone's curve leaves the original");
            Expect.That(!ReferenceEquals(clone.tint, original.tint) && clone.tint.Equals(original.tint), "the clone has an equal gradient of its own");
            Expect.Equal(GradientMode.Fixed, clone.tint.mode, "the gradient mode is copied");
            Expect.Equal(ColorSpace.Linear, clone.tint.colorSpace, "the gradient color space is copied");
            Expect.Sequence(original.tint.alphaKeys, clone.tint.alphaKeys, "the alpha keys are copied");
            Expect.That(clone.steps.Length == 1 && !ReferenceEquals(clone.steps[0], original.steps[0]) && clone.steps[0].keys[0].value == 5, "curves in an array are copied");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: Instantiate copies serialized AnimationCurve and Gradient fields into objects of their own");
    }
}
