using Disunity.Hosting;
using UnityEngine;

/// <summary>The secondary textures of a CanvasRenderer: a count and named slots that are stored and never drawn.</summary>
internal static class CanvasSecondaryTextureTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            CountAndSlots();
            Console.WriteLine("PASS: CanvasRenderer secondary textures: the count, named slots, bounds and a negative count");
            LoweringTheCountDropsSlots();
            Console.WriteLine("PASS: lowering the secondary texture count drops the slots past it and Clear empties them");
            CloneHasItsOwnSlots();
            Console.WriteLine("PASS: a cloned CanvasRenderer keeps the secondary textures and edits them apart");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static CanvasRenderer NewRenderer() => DisunityHost.CreateGameObject().AddComponent<CanvasRenderer>();

    private static void CountAndSlots()
    {
        var renderer = NewRenderer();
        Expect.Equal(0, renderer.SecondaryTextureCount, "no secondary textures at first");
        Expect.Throws<ArgumentOutOfRangeException>(() => renderer.SetSecondaryTextureCount(-1), "a negative count");
        Expect.Throws<ArgumentOutOfRangeException>(() => renderer.SetSecondaryTexture(0, "_Normal", new Texture2D(1, 1)), "a slot of an empty list");
        renderer.SetSecondaryTextureCount(2);
        Expect.Equal(2, renderer.SecondaryTextureCount, "the count is stored");
        Expect.That(renderer.SecondaryTextureAt(1) is (null, null), "a new slot is empty");
        var texture = new Texture2D(2, 2);
        renderer.SetSecondaryTexture(1, "_Normal", texture);
        var (name, held) = renderer.SecondaryTextureAt(1);
        Expect.That(name == "_Normal" && ReferenceEquals(held, texture), "the slot keeps the name and texture");
        Expect.That(renderer.SecondaryTextureAt(0) is (null, null), "the other slot is untouched");
        var atCount = Expect.Catch<ArgumentOutOfRangeException>(() => renderer.SetSecondaryTexture(2, "_X", texture), "an index at the count");
        Expect.That(atCount.Message.Contains("2 secondary texture slots"), $"an index at the count names the slot count: {atCount.Message}");
        Expect.Throws<ArgumentOutOfRangeException>(() => renderer.SetSecondaryTexture(-1, "_X", texture), "a negative index");
        renderer.SetSecondaryTexture(0, "_Mask", null);
        Expect.That(renderer.SecondaryTextureAt(0) is ("_Mask", null), "a null texture clears the texture and keeps the name");
    }

    private static void LoweringTheCountDropsSlots()
    {
        var renderer = NewRenderer();
        var texture = new Texture2D(2, 2);
        renderer.SetSecondaryTextureCount(3);
        renderer.SetSecondaryTexture(0, "_A", texture);
        renderer.SetSecondaryTexture(2, "_C", texture);
        renderer.SetSecondaryTextureCount(1);
        Expect.Equal(1, renderer.SecondaryTextureCount, "the count drops");
        Expect.That(renderer.SecondaryTextureAt(0).Name == "_A", "the slot below the count stays");
        renderer.SetSecondaryTextureCount(3);
        Expect.That(renderer.SecondaryTextureAt(2) is (null, null), "a slot that came back is empty");
        renderer.Clear();
        Expect.Equal(0, renderer.SecondaryTextureCount, "Clear empties the secondary textures");
    }

    private static void CloneHasItsOwnSlots()
    {
        var renderer = NewRenderer();
        var texture = new Texture2D(2, 2);
        renderer.SetSecondaryTextureCount(1);
        renderer.SetSecondaryTexture(0, "_A", texture);
        var clone = UnityEngine.Object.Instantiate(renderer);
        Expect.That(clone.SecondaryTextureCount == 1 && clone.SecondaryTextureAt(0).Name == "_A", "the clone has the slots");
        clone.SetSecondaryTexture(0, "_B", null);
        Expect.Equal("_A", renderer.SecondaryTextureAt(0).Name, "editing the clone leaves the source");
    }
}
