using System.Reflection;
using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.U2D;

/// <summary>A player's textures, sprites and sprite atlases loaded through a resource plan, and Sprite.Create.</summary>
internal static class PlayerSpriteTests
{
    private const int SheetBytes = 8 * 8 * 4;
    private const int Offset = 64;

    public static void Run()
    {
        PlanHarness.WithPlan(WritePlan, () =>
        {
            LoadsATextureWithItsSettingsAndBytes();
            ChecksTheByteCountAgainstTheFormat();
            LoadsASpriteOverItsTexture();
            AtlasKeepsThePlannedOrder();
            LoadAllReturnsTheTextureAndItsSprites();
            TexturesKeepOneIdentityAndPlayerIdentitiesStayClearOfSceneOnes();
        });
        SpriteCreateChecksItsArguments();
        SpriteCreateMembers();
    }

    private static byte[] FileBytes() => Enumerable.Range(0, Offset + SheetBytes).Select(i => (byte)(i % 251)).ToArray();

    private static JsonObject Texture(string name, string file, int size, bool readable) => new()
    {
        ["type"] = "Texture2D", ["name"] = name, ["width"] = 8, ["height"] = 8, ["format"] = 4, ["mip_count"] = 1,
        ["readable"] = readable, ["color_space"] = 1, ["filter_mode"] = 0, ["wrap_u"] = 1, ["wrap_v"] = 0, ["wrap_w"] = 2,
        ["aniso_level"] = 4, ["mip_bias"] = -0.5,
        ["data"] = new JsonObject { ["file"] = file, ["offset"] = Offset, ["size"] = size },
    };

    private static JsonArray Floats(params float[] values) => new(values.Select(v => (JsonNode)JsonValue.Create(v)!).ToArray());

    private static JsonObject PlannedSprite(string name, float[] rect, float[] pivot, float ppu, float[] textureRect, float[] offset) => new()
    {
        ["type"] = "Sprite", ["name"] = name, ["rect"] = Floats(rect), ["pivot"] = Floats(pivot), ["pixels_per_unit"] = ppu,
        ["border"] = Floats(1, 2, 3, 4), ["extrude"] = 1, ["texture"] = new JsonObject { ["$asset"] = 0 },
        ["texture_rect"] = Floats(textureRect), ["texture_rect_offset"] = Floats(offset), ["packed"] = !rect.SequenceEqual(textureRect),
        ["packing_rotation"] = 0, ["mesh_type"] = 1,
    };

    private static string WritePlan(string directory)
    {
        File.WriteAllBytes(Path.Combine(directory, "sheet.bin"), FileBytes());
        File.WriteAllBytes(Path.Combine(directory, "short.bin"), new byte[100]);
        var assets = new JsonArray
        {
            Texture("sheet", "sheet.bin", SheetBytes, readable: true),
            PlannedSprite("a", [2, 4, 4, 2], [0.5f, 0.25f], 50, [2, 4, 4, 2], [0, 0]),
            PlannedSprite("b", [0, 0, 8, 8], [0.5f, 0.5f], 100, [1, 1, 6, 6], [1, 1]),
            new JsonObject
            {
                ["type"] = "SpriteAtlas", ["name"] = "icons", ["tag"] = "ui", ["is_variant"] = true,
                ["sprites"] = new JsonArray(new JsonObject { ["$asset"] = 2 }, new JsonObject { ["$asset"] = 1 }),
            },
            Texture("bad", "short.bin", 100, readable: true),
            Texture("locked", "sheet.bin", SheetBytes, readable: false),
        };
        string[] paths = ["art/sheet", "art/sheet", "art/sheet", "art/atlas", "art/bad", "art/locked"];
        string[] classes = ["Texture2D", "Sprite", "Sprite", "SpriteAtlas", "Texture2D", "Texture2D"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static void LoadsATextureWithItsSettingsAndBytes()
    {
        var texture = Resources.Load<Texture2D>("art/sheet")!;
        Expect.That(texture.name == "sheet" && texture.width == 8 && texture.height == 8 && texture.format == TextureFormat.RGBA32 && texture.mipmapCount == 1,
            "a planned texture has its name, size, format and mip count");
        Expect.That(texture.isReadable && texture.filterMode == FilterMode.Point && texture.wrapModeU == TextureWrapMode.Clamp
            && texture.wrapModeV == TextureWrapMode.Repeat && texture.wrapModeW == TextureWrapMode.Mirror && texture.anisoLevel == 4 && texture.mipMapBias == -0.5f,
            "a planned texture has its readable flag and sampler settings");
        Expect.That(texture.isDataSRGB, "color space 1 is sRGB");
        Expect.Sequence(FileBytes().Skip(Offset), texture.GetRawTextureData(), "a planned texture holds the bytes at its offset");
        var locked = Resources.Load<Texture2D>("art/locked")!;
        Expect.That(!locked.isReadable, "a texture planned as not readable is not readable");
        Console.WriteLine("PASS: a planned texture loads with its size, format, mips, settings and bytes");
    }

    private static void ChecksTheByteCountAgainstTheFormat()
    {
        var error = ThrownBy(() => Resources.Load<Texture2D>("art/bad"));
        Expect.That(error is InvalidDataException && error.Message.Contains("bad") && error.Message.Contains("100") && error.Message.Contains(SheetBytes.ToString()),
            $"a wrong byte count fails naming the texture and both sizes, got {error?.GetType().Name}: {error?.Message}");
        Console.WriteLine("PASS: a planned texture whose byte count does not fit its format fails naming both sizes");
    }

    private static Exception? ThrownBy(Action action)
    {
        try { action(); }
        catch (Exception error) { return error; }
        return null;
    }

    private static void LoadsASpriteOverItsTexture()
    {
        var texture = Resources.Load<Texture2D>("art/sheet");
        var sprite = Resources.LoadAll<Sprite>("art/sheet").First(s => s.name == "a");
        Expect.That(sprite.rect == new Rect(2, 4, 4, 2) && sprite.pixelsPerUnit == 50 && sprite.pivot == new Vector2(2, 0.5f), "a planned sprite has its rect, pixels per unit and pivot in pixels");
        Expect.That(sprite.border == new Vector4(1, 2, 3, 4) && sprite.textureRect == new Rect(2, 4, 4, 2) && sprite.textureRectOffset == Vector2.zero && !sprite.packed,
            "a planned sprite has its border, texture rect, offset and packed flag");
        Expect.That(ReferenceEquals(sprite.texture, texture), "a planned sprite's texture is the Texture2D that loading the texture returns");
        var packed = Resources.LoadAll<Sprite>("art/sheet").First(s => s.name == "b");
        Expect.That(packed.packed && packed.rect == new Rect(0, 0, 8, 8) && packed.textureRect == new Rect(1, 1, 6, 6) && packed.textureRectOffset == new Vector2(1, 1),
            "a packed sprite reports its own rect apart from where its pixels are");
        var scene = DisunityHost.CreateSprite(1, 8, 8, new Rect(0, 0, 4, 4), new Vector2(0.5f, 0.5f));
        Expect.That(scene.texture is null && scene.textureRect == scene.rect && scene.border == Vector4.zero && !scene.packed,
            "a sprite of the scene has no managed texture, its texture rect is its rect, and it has no border");
        Console.WriteLine("PASS: a planned sprite loads with its rect, pivot, border, texture and texture rect");
    }

    private static void AtlasKeepsThePlannedOrder()
    {
        var atlas = Resources.Load<SpriteAtlas>("art/atlas")!;
        var sprites = new Sprite[3];
        Expect.That(atlas.name == "icons" && atlas.tag == "ui" && atlas.isVariant && atlas.spriteCount == 2 && atlas.GetSprites(sprites) == 2, "a planned atlas has its name, tag, variant flag and sprite count");
        Expect.Sequence(["b", "a"], sprites.Take(2).Select(s => s.name), "an atlas returns its sprites in the planned order");
        Expect.That(atlas.CanBindTo(Resources.LoadAll<Sprite>("art/sheet").First(s => s.name == "a")), "the atlas's sprites are the ones the sheet loads");
        Console.WriteLine("PASS: a planned atlas returns its sprites in order");
    }

    private static void LoadAllReturnsTheTextureAndItsSprites()
    {
        Expect.Sequence(["a", "b"], Resources.LoadAll<Sprite>("art/sheet").Select(s => s.name), "LoadAll<Sprite> returns the sprites listed under the path");
        var all = Resources.LoadAll("art/sheet");
        Expect.That(all.Length == 3 && all[0] is Texture2D && all[1] is Sprite && all[2] is Sprite, "LoadAll returns the texture and the sprites in container order");
        Expect.That(Resources.LoadAll<Texture2D>("art/sheet").Length == 1, "LoadAll<Texture2D> returns the texture only");
        Console.WriteLine("PASS: LoadAll over a path with a texture and two sprites");
    }

    private static ulong TextureIdentity(Sprite sprite)
    {
        var description = typeof(Sprite).GetField("Description", BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(sprite)!;
        return (ulong)description.GetType().GetField("texture")!.GetValue(description)!;
    }

    private static void TexturesKeepOneIdentityAndPlayerIdentitiesStayClearOfSceneOnes()
    {
        var a = Resources.LoadAll<Sprite>("art/sheet").First(s => s.name == "a");
        var b = Resources.LoadAll<Sprite>("art/sheet").First(s => s.name == "b");
        var locked = Resources.Load<Texture2D>("art/locked")!;
        var other = Sprite.Create(locked, new Rect(0, 0, 2, 2), new Vector2(0.5f, 0.5f));
        Expect.That(TextureIdentity(a) == TextureIdentity(b), "sprites of one texture share its identity");
        Expect.That(TextureIdentity(other) != TextureIdentity(a), "another texture has another identity");
        Expect.That(TextureIdentity(a) >= 1UL << 40 && TextureIdentity(other) >= 1UL << 40, "player textures take identities from 2^40, above any scene texture");
        Expect.That(TextureIdentity(Sprite.Create(locked, new Rect(2, 2, 2, 2), Vector2.zero)) == TextureIdentity(other), "a texture keeps its identity for its lifetime");
        Console.WriteLine("PASS: player textures keep one identity each, in a range clear of scene textures");
    }

    private static void SpriteCreateChecksItsArguments()
    {
        DisunityHost.LoadWorld();
        try
        {
            var texture = new Texture2D(8, 4, TextureFormat.RGBA32, false);
            var pivot = new Vector2(0.5f, 0.5f);
            Expect.Throws<ArgumentNullException>(() => Sprite.Create(null!, new Rect(0, 0, 2, 2), pivot), "a null texture");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(0, 0, 0, 2), pivot), "a rect of no width");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(0, 0, 2, -1), pivot), "a rect of negative height");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(-1, 0, 2, 2), pivot), "a rect left of the texture");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(7, 0, 2, 2), pivot), "a rect right of the texture");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(0, 3, 2, 2), pivot), "a rect above the texture");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(0, 0, 2, 2), pivot, 0), "pixels per unit of zero");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(0, 0, 2, 2), pivot, float.NaN), "pixels per unit of NaN");
            Expect.Throws<ArgumentException>(() => Sprite.Create(texture, new Rect(0, 0, 2, 2), pivot, float.PositiveInfinity), "infinite pixels per unit");
            Expect.That(Sprite.Create(texture, new Rect(6, 2, 2, 2), pivot) is { } edge && edge.rect == new Rect(6, 2, 2, 2), "a rect that touches the texture's far edges is inside it");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: Sprite.Create rejects a null texture, an empty or outside rect and bad pixels per unit");
    }

    private static void SpriteCreateMembers()
    {
        DisunityHost.LoadWorld();
        try
        {
            var texture = new Texture2D(8, 4, TextureFormat.RGBA32, false);
            var rect = new Rect(2, 0, 4, 4);
            var sprite = Sprite.Create(texture, rect, new Vector2(0.25f, 0.5f));
            Expect.That(sprite.pixelsPerUnit == 100 && sprite.rect == rect && sprite.pivot == new Vector2(1, 2) && sprite.textureRect == rect && sprite.textureRectOffset == Vector2.zero,
                "Sprite.Create takes 100 pixels per unit and a pivot as a fraction of the rect, which pivot returns in pixels");
            Expect.That(ReferenceEquals(sprite.texture, texture) && sprite.border == Vector4.zero && !sprite.packed, "a created sprite keeps its texture and has no border");
            var full = Sprite.Create(texture, rect, Vector2.zero, 16, 2, SpriteMeshType.Tight, new Vector4(1, 1, 1, 1));
            Expect.That(full.pixelsPerUnit == 16 && full.border == new Vector4(1, 1, 1, 1), "the longest overload sets pixels per unit and the border");
            Expect.That(Sprite.Create(texture, rect, Vector2.zero, 32).pixelsPerUnit == 32 && Sprite.Create(texture, rect, Vector2.zero, 32, 1).pixelsPerUnit == 32
                && Sprite.Create(texture, rect, Vector2.zero, 8, 1, SpriteMeshType.FullRect).pixelsPerUnit == 8, "the shorter overloads set pixels per unit");
            var holder = new GameObject().AddComponent<SpriteRenderer>();
            holder.sprite = sprite;
            Expect.That(holder.sprite is { } read && ReferenceEquals(read.texture, texture) && read.rect == rect, "a sprite read back from a renderer still has its texture");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: Sprite.Create members and overloads");
    }
}
