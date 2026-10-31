using Disunity.Hosting;
using UnityEngine;

/// <summary>Instantiate copies a serialized RectOffset field: the clone has the same offsets in its own object.</summary>
internal static class RectOffsetCloneTests
{
    private sealed class Padded : MonoBehaviour
    {
        public RectOffset padding = new(1, 2, 3, 4);
        public RectOffset? none = null;
    }

    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var owner = new GameObject("padded");
            var original = owner.AddComponent<Padded>();
            var clone = UnityEngine.Object.Instantiate(owner).GetComponent<Padded>()!;
            Expect.That(!ReferenceEquals(clone.padding, original.padding), "the clone has its own RectOffset");
            Expect.That(clone.padding is { left: 1, right: 2, top: 3, bottom: 4 }, $"the offsets are copied, got {clone.padding}");
            clone.padding.left = 9;
            Expect.Equal(1, original.padding.left, "changing the clone leaves the original");
            Expect.That(clone.none is null, "a null RectOffset stays null");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: Instantiate copies a serialized RectOffset field into its own object");
    }
}
