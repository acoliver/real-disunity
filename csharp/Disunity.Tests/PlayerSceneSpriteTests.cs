using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>Sprites a player scene plan holds as assets: built with the texture they draw, whose pixels the plan names in
/// place in a file by absolute path, and shared by every script field that references them.</summary>
internal static class PlayerSceneSpriteTests
{
    private const int Offset = 64;
    private const int SheetBytes = 8 * 8 * 4;

    public sealed class SpriteHolder : MonoBehaviour
    {
        public static SpriteHolder? Seen;
        public Sprite? first = null;
        public Sprite? again = null;
        public Sprite? sibling = null;
        public Sprite? missing = null;
        private void Awake() => Seen = this;
    }

    public static void Run()
    {
        var directory = Path.Combine(Path.GetTempPath(), $"disunity-scene-sprites-{Guid.NewGuid():N}");
        Directory.CreateDirectory(directory);
        try
        {
            var file = Path.Combine(directory, "sharedassets.bin");
            File.WriteAllBytes(file, Enumerable.Range(0, Offset + SheetBytes).Select(i => (byte)(i % 251)).ToArray());
            BuildsTheSpritesOverTheirTextureFromTheFile(file);
            ASpriteOfAMissingTextureFileFailsTheLoad(Path.Combine(directory, "gone.bin"));
        }
        finally { Directory.Delete(directory, recursive: true); }
    }

    private static JsonArray Floats(params float[] values) => new(values.Select(v => (JsonNode)JsonValue.Create(v)!).ToArray());

    private static JsonObject Texture(string file) => new()
    {
        ["type"] = "Texture2D", ["name"] = "sheet", ["width"] = 8, ["height"] = 8, ["format"] = 4, ["mip_count"] = 1,
        ["readable"] = true, ["color_space"] = 1, ["filter_mode"] = 1, ["wrap_u"] = 1, ["wrap_v"] = 0, ["wrap_w"] = 2,
        ["aniso_level"] = 1, ["mip_bias"] = 0.0,
        ["data"] = new JsonObject { ["file"] = file, ["offset"] = Offset, ["size"] = SheetBytes },
    };

    private static JsonObject PlannedSprite(string name, float[] rect) => new()
    {
        ["type"] = "Sprite", ["name"] = name, ["rect"] = Floats(rect), ["pivot"] = Floats(0.5f, 0.5f), ["pixels_per_unit"] = 50,
        ["border"] = Floats(0, 0, 0, 0), ["extrude"] = 1, ["texture"] = new JsonObject { ["$asset"] = 0 },
        ["texture_rect"] = Floats(rect), ["texture_rect_offset"] = Floats(0, 0), ["packed"] = false,
        ["packing_rotation"] = 0, ["mesh_type"] = 1,
    };

    private static string Plan(string file)
    {
        var holder = new JsonObject
        {
            ["name"] = "Holder", ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = null, ["template"] = false,
            ["position"] = Floats(0, 0, 0), ["rotation"] = Floats(0, 0, 0, 1), ["scale"] = Floats(1, 1, 1), ["rect"] = null,
            ["components"] = new JsonArray(new JsonObject
            {
                ["type"] = "Script", ["class"] = "PlayerSceneSpriteTests/SpriteHolder", ["assembly"] = typeof(PlayerSceneSpriteTests).Assembly.GetName().Name,
                ["enabled"] = true,
                ["fields"] = new JsonObject
                {
                    ["first"] = new JsonObject { ["$asset"] = 1 }, ["again"] = new JsonObject { ["$asset"] = 1 },
                    ["sibling"] = new JsonObject { ["$asset"] = 2 }, ["missing"] = null,
                },
            }),
        };
        return new JsonObject
        {
            ["scene"] = "loading", ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
            ["differences"] = new JsonArray(), ["objects"] = new JsonArray(holder),
            ["assets"] = new JsonArray(Texture(file), PlannedSprite("a", [2, 4, 4, 2]), PlannedSprite("b", [0, 0, 8, 8])),
        }.ToJsonString();
    }

    private static LoadedScene Load(string plan)
    {
        var assembly = typeof(PlayerSceneSpriteTests).Assembly;
        return DisunityHost.LoadScene(plan, new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
    }

    private static void BuildsTheSpritesOverTheirTextureFromTheFile(string file)
    {
        DisunityHost.LoadWorld();
        try
        {
            using var scene = Load(Plan(file));
            var seen = SpriteHolder.Seen!;
            Expect.That(seen.first is not null && ReferenceEquals(seen.first, seen.again) && seen.sibling is not null && !ReferenceEquals(seen.first, seen.sibling) && seen.missing is null,
                "two fields that name one planned sprite hold one Sprite, and another sprite is another object");
            Expect.That(seen.first!.name == "a" && seen.first.rect == new Rect(2, 4, 4, 2) && seen.first.pixelsPerUnit == 50 && seen.first.pivot == new Vector2(2, 1),
                "a scene sprite has its name, rect, pixels per unit and pivot");
            var texture = seen.first.texture;
            Expect.That(texture is not null && ReferenceEquals(texture, seen.sibling!.texture) && texture.width == 8 && texture.height == 8 && texture.filterMode == FilterMode.Bilinear,
                "sprites of one planned texture share the Texture2D, which has its size and sampler");
            Expect.Sequence(Enumerable.Range(Offset, SheetBytes).Select(i => (byte)(i % 251)), texture!.GetRawTextureData(),
                "the scene texture holds the bytes at its offset of the file the plan names");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: a scene plan builds its sprites over one texture read from the file the plan names");
    }

    private static void ASpriteOfAMissingTextureFileFailsTheLoad(string file)
    {
        DisunityHost.LoadWorld();
        try
        {
            Exception? error = null;
            try { Load(Plan(file)).Dispose(); }
            catch (Exception caught) { error = caught; }
            Expect.That(error is not null && Describe(error).Contains("gone.bin"), $"a texture file that is gone fails the load naming it, got {error?.Message}");
        }
        finally { DisunityHost.UnloadWorld(); }
        Console.WriteLine("PASS: a scene texture whose file is gone fails the scene load naming the file");
    }

    private static string Describe(Exception error) => error.InnerException is { } inner ? $"{error.Message} {Describe(inner)}" : error.Message;
}
