using Disunity.Hosting;
using UnityEngine;
using UnityEngine.Sprites;

/// <summary>The UVs, padding and minimum size of a sprite.</summary>
internal static class SpriteDataUtilityTests
{
    public static void Run()
    {
        DisunityHost.LoadWorld();
        try
        {
            WithBorder();
            Console.WriteLine("PASS: DataUtility gives the outer UV, the border-inset inner UV, zero padding and the border sums for a bordered sprite");
            WithoutBorder();
            Console.WriteLine("PASS: DataUtility gives equal inner and outer UV and a zero minimum size for a sprite without a border");
            Packed();
            Console.WriteLine("PASS: DataUtility reports the padding of a packed sprite and counts it toward the border");
            WithoutTexture();
            Console.WriteLine("PASS: DataUtility gives the unit UV and zero padding for a sprite without a texture");
            NullSprite();
            Console.WriteLine("PASS: DataUtility rejects a null sprite");
        }
        finally { DisunityHost.UnloadWorld(); }
    }

    private static void WithBorder()
    {
        var texture = new Texture2D(8, 4, TextureFormat.RGBA32, false);
        var sprite = Sprite.Create(texture, new Rect(2, 0, 4, 4), Vector2.zero, 100, 0, SpriteMeshType.Tight, new Vector4(1, 1, 1, 1));
        Expect.Equal(new Vector4(0.25f, 0, 0.75f, 1), DataUtility.GetOuterUV(sprite), "outer UV");
        Expect.Equal(new Vector4(0.375f, 0.25f, 0.625f, 0.75f), DataUtility.GetInnerUV(sprite), "inner UV");
        Expect.Equal(Vector4.zero, DataUtility.GetPadding(sprite), "padding");
        Expect.Equal(new Vector2(2, 2), DataUtility.GetMinSize(sprite), "min size");
        var uneven = Sprite.Create(texture, new Rect(0, 0, 8, 4), Vector2.zero, 100, 0, SpriteMeshType.Tight, new Vector4(2, 1, 0, 3));
        Expect.Equal(new Vector4(0.25f, 0.25f, 1, 0.25f), DataUtility.GetInnerUV(uneven), "inner UV with different borders");
        Expect.Equal(new Vector2(2, 4), DataUtility.GetMinSize(uneven), "min size with different borders");
    }

    private static void WithoutBorder()
    {
        var texture = new Texture2D(8, 4, TextureFormat.RGBA32, false);
        var sprite = Sprite.Create(texture, new Rect(4, 2, 4, 2), Vector2.zero);
        var outer = DataUtility.GetOuterUV(sprite);
        Expect.Equal(new Vector4(0.5f, 0.5f, 1, 1), outer, "outer UV");
        Expect.Equal(outer, DataUtility.GetInnerUV(sprite), "inner UV equals outer UV");
        Expect.Equal(Vector2.zero, DataUtility.GetMinSize(sprite), "min size");
    }

    private static void Packed()
    {
        var texture = new Texture2D(8, 8, TextureFormat.RGBA32, false);
        var details = new SpriteDetails(new Rect(0, 0, 8, 8), new Rect(1, 1, 6, 6), new Vector2(1, 1), new Vector4(2, 0, 0, 0), true);
        var sprite = DisunityHost.CreateSprite(texture, details, Vector2.zero, 100);
        Expect.Equal(new Vector4(1, 1, 1, 1), DataUtility.GetPadding(sprite), "padding");
        Expect.Equal(new Vector4(0.125f, 0.125f, 0.875f, 0.875f), DataUtility.GetOuterUV(sprite), "outer UV");
        Expect.Equal(new Vector4(0.25f, 0.125f, 0.875f, 0.875f), DataUtility.GetInnerUV(sprite), "inner UV counts padding toward the border");
    }

    private static void WithoutTexture()
    {
        var sprite = DisunityHost.CreateSprite(1, 8, 8, new Rect(0, 0, 4, 4), new Vector2(0.5f, 0.5f));
        Expect.Equal(new Vector4(0, 0, 1, 1), DataUtility.GetOuterUV(sprite), "outer UV");
        Expect.Equal(new Vector4(0, 0, 1, 1), DataUtility.GetInnerUV(sprite), "inner UV");
        Expect.Equal(Vector4.zero, DataUtility.GetPadding(sprite), "padding");
        Expect.Equal(Vector2.zero, DataUtility.GetMinSize(sprite), "min size");
    }

    private static void NullSprite()
    {
        Expect.Throws<ArgumentNullException>(() => DataUtility.GetInnerUV(null!), "inner UV");
        Expect.Throws<ArgumentNullException>(() => DataUtility.GetOuterUV(null!), "outer UV");
        Expect.Throws<ArgumentNullException>(() => DataUtility.GetPadding(null!), "padding");
        Expect.Throws<ArgumentNullException>(() => DataUtility.GetMinSize(null!), "min size");
    }
}
