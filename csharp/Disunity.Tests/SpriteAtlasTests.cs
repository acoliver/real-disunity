using System.Reflection;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.U2D;
using Object = UnityEngine.Object;

/// <summary>
/// Sprites as Objects (a name, an instance ID, generic Resources calls) and the SpriteAtlas that
/// holds a group of them. The atlas has an internal constructor because the engine does not load
/// atlases from player data yet, so the test reaches it by reflection.
/// </summary>
internal static class SpriteAtlasTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            var first = DisunityHost.CreateSprite(1, 8, 8, new Rect(0, 0, 2, 4), new Vector2(0.5f, 0.5f), 2);
            var second = DisunityHost.CreateSprite(1, 8, 8, new Rect(2, 0, 4, 4), new Vector2(0, 1), 4);
            first.name = "first";
            second.name = "second";
            SpriteIsAnObject(first, second);
            var sprites = new List<Sprite> { first, second };
            var atlas = NewAtlas("group", "tagged", true, sprites);
            Expect.That(atlas is Object { name: "group" }, "an atlas is an Object with the name it was made with");
            Expect.That(atlas.spriteCount == 2 && atlas.tag == "tagged" && atlas.isVariant, "atlas count, tag and variant flag");
            sprites.Add(first);
            Expect.Equal(2, atlas.spriteCount, "an atlas keeps the sprites it was made with, not the caller's later changes");
            GetSpritesFillsFromTheStart(atlas, first, second);
            GetSpritesCopies(atlas, first);
            FindsAndBindsItsOwnSprites(atlas, first, second);
            Expect.That(!NewAtlas("empty", "", false, []).isVariant && NewAtlas("empty", "", false, []).spriteCount == 0, "an empty atlas has no sprites");
            AtlasAndTextCoreTypesForwardFromTheirModules();
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: sprites are Objects and SpriteAtlas reports, copies and finds its sprites");
    }

    private static SpriteAtlas NewAtlas(string name, string tag, bool isVariant, IReadOnlyList<Sprite> sprites) =>
        (SpriteAtlas)Activator.CreateInstance(typeof(SpriteAtlas), BindingFlags.Instance | BindingFlags.NonPublic, null,
            [name, tag, isVariant, sprites], null)!;

    private static void SpriteIsAnObject(Sprite first, Sprite second)
    {
        Object asObject = first;
        Expect.That(asObject.name == "first" && first.GetInstanceID() < 0 && first.GetInstanceID() % 2 == 0
            && first.GetInstanceID() != second.GetInstanceID(), "a sprite has a name and its own even, negative instance ID");
        Expect.That(ReferenceEquals(Resources.InstanceIDToObject(first.GetInstanceID()), first), "a sprite is found by its instance ID");
        Expect.That(DisunityHost.CreateSprite(9, 4, 4, new Rect(0, 0, 4, 4), new Vector2(0.5f, 0.5f)).name == "", "a sprite made without a name has an empty one");
        Expect.That(Resources.LoadAll<Sprite>("").Length == 0 && Resources.Load<Sprite>("none") is null, "the generic Resources calls accept Sprite and find nothing without resources");
        var doomed = DisunityHost.CreateSprite(10, 4, 4, new Rect(0, 0, 4, 4), new Vector2(0.5f, 0.5f));
        Object.Destroy(doomed);
        Expect.That(doomed == null, "a destroyed sprite compares equal to null");
    }

    private static void GetSpritesFillsFromTheStart(SpriteAtlas atlas, Sprite first, Sprite second)
    {
        Expect.Throws<ArgumentNullException>(() => atlas.GetSprites(null!), "GetSprites(null)");
        var roomy = new Sprite[3];
        Expect.Equal(2, atlas.GetSprites(roomy), "GetSprites returns the number written");
        Expect.That(roomy[0]?.name == "first" && roomy[1]?.name == "second" && roomy[2] is null, "an array longer than the atlas is filled from its start and left otherwise");
        Expect.That(roomy[0].rect == first.rect && roomy[1].rect == second.rect && roomy[1].pivot == second.pivot
            && roomy[1].pixelsPerUnit == 4, "the returned sprites have the atlas sprites' content");
        var tight = new Sprite[1];
        Expect.That(atlas.GetSprites(tight) == 1 && tight[0].name == "first", "an array shorter than the atlas takes the first sprites");
        Expect.Equal(0, atlas.GetSprites([]), "an empty array takes nothing");
    }

    private static void GetSpritesCopies(SpriteAtlas atlas, Sprite first)
    {
        var one = new Sprite[2];
        var two = new Sprite[2];
        atlas.GetSprites(one);
        atlas.GetSprites(two);
        Expect.That(!ReferenceEquals(one[0], first) && !ReferenceEquals(one[0], two[0]), "GetSprites returns new Sprite objects on every call");
        one[0].name = "renamed";
        one[1].name = "renamed";
        var again = new Sprite[2];
        atlas.GetSprites(again);
        Expect.That(first.name == "first" && again[0].name == "first" && again[1].name == "second", "renaming a returned sprite leaves the atlas's own sprites alone");
    }

    private static void FindsAndBindsItsOwnSprites(SpriteAtlas atlas, Sprite first, Sprite second)
    {
        Expect.That(ReferenceEquals(atlas.GetSprite("second"), second) && ReferenceEquals(atlas.GetSprite("first"), first), "GetSprite finds the sprite with that name");
        Expect.That(atlas.GetSprite("missing") is null, "GetSprite finds nothing for an unknown name");
        Expect.Throws<ArgumentNullException>(() => atlas.GetSprite(null!), "GetSprite(null)");
        Expect.That(atlas.CanBindTo(first) && atlas.CanBindTo(second), "CanBindTo accepts the atlas's sprites");
        var other = DisunityHost.CreateSprite(11, 4, 4, new Rect(0, 0, 4, 4), new Vector2(0.5f, 0.5f));
        var copy = new Sprite[1];
        atlas.GetSprites(copy);
        Expect.That(!atlas.CanBindTo(other) && !atlas.CanBindTo(copy[0]) && !atlas.CanBindTo(null!), "CanBindTo refuses other sprites, copies and null");
    }

    private static void AtlasAndTextCoreTypesForwardFromTheirModules()
    {
        var engine = typeof(DisunityHost).Assembly;
        foreach (var name in new[]
        {
            "UnityEngine.U2D.SpriteAtlas", "UnityEngine.TextCore.FaceInfo", "UnityEngine.TextCore.Glyph", "UnityEngine.TextCore.GlyphRect",
            "UnityEngine.TextCore.GlyphMetrics", "UnityEngine.TextCore.LowLevel.GlyphValueRecord", "UnityEngine.TextCore.LowLevel.GlyphAdjustmentRecord",
            "UnityEngine.TextCore.LowLevel.GlyphPairAdjustmentRecord", "UnityEngine.TextCore.LowLevel.GlyphAnchorPoint",
            "UnityEngine.TextCore.LowLevel.MarkPositionAdjustment", "UnityEngine.TextCore.LowLevel.MarkToBaseAdjustmentRecord",
            "UnityEngine.TextCore.LowLevel.MarkToMarkAdjustmentRecord", "UnityEngine.TextCore.LowLevel.LigatureSubstitutionRecord",
        })
            Expect.That(ModuleFacades.Forwards(engine, name), $"{name} is public, so its module facade forwards it");
    }
}
