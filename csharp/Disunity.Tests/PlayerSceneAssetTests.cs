using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;
using UnityEngine.TextCore.LowLevel;

/// <summary>Script assets and fonts a player's scene plan holds: a scene script's field names the asset, which
/// is enabled before the scene's scripts, holds its own fields and the font it names, and may name assets that
/// name it back.</summary>
internal static class PlayerSceneAssetTests
{
    public sealed class Settings : ScriptableObject
    {
        public static Settings? Instance;
        public int count = 0;
        public Settings? next = null;
        public Font? face = null;
        private void OnEnable() => Instance = this;
    }

    public sealed class Holder : MonoBehaviour
    {
        public static Holder? Seen;
        public Settings? settings = null;
        public Settings? again = null;
        public Settings? instanceAtEnable;
        private void OnEnable() { Seen = this; instanceAtEnable = Settings.Instance; }
    }

    public static void Run()
    {
        var directory = Directory.CreateTempSubdirectory("disunity-scene-assets-");
        try
        {
            var font = Path.Combine(directory.FullName, "fonts.bin");
            var bytes = SyntheticFont.Bytes();
            File.WriteAllBytes(font, [7, 7, .. bytes, 9]);
            DisunityHost.LoadWorld();
            try
            {
                var assembly = typeof(PlayerSceneAssetTests).Assembly;
                var scene = DisunityHost.LoadScene(ScenePlan(font, bytes.Length), new SceneLoadOptions(assembly, _ => []) { Assemblies = _ => assembly });
                try { SceneHoldsTheAssets(); }
                finally { scene.Dispose(); }
            }
            finally { DisunityHost.UnloadWorld(); }
        }
        finally { directory.Delete(recursive: true); }
    }

    private static void SceneHoldsTheAssets()
    {
        var holder = Holder.Seen!;
        var first = holder.settings!;
        Expect.That(first is { count: 1, name: "first" } && ReferenceEquals(holder.again, first), "a scene script holds the planned asset, two references to it being one object");
        Expect.That(first.next is { count: 2, name: "second" } && ReferenceEquals(first.next.next, first), "assets that name each other are built once each");
        Expect.That(ReferenceEquals(Settings.Instance, first.next) && ReferenceEquals(holder.instanceAtEnable, first.next), "the assets are enabled before the scene's scripts");
        Expect.That(first.face is { name: "Sans" } && ReferenceEquals(first.face, first.next!.face), "a font the assets name is one object");
        Expect.Equal(FontEngineError.Success, FontEngine.LoadFontFace(first.face!, 40), "a face loads from the font's range of the data file");
        Console.WriteLine("PASS: a player scene plan builds the script assets and fonts its scripts reference");
    }

    private static JsonObject Settings_(string name, int count, int next) => new()
    {
        ["type"] = "Script", ["script"] = "", ["class"] = "PlayerSceneAssetTests/Settings", ["assembly"] = typeof(PlayerSceneAssetTests).Assembly.GetName().Name,
        ["name"] = name, ["fields"] = new JsonObject
        {
            ["count"] = count.ToString(), ["next"] = new JsonObject { ["$asset"] = next }, ["face"] = new JsonObject { ["$asset"] = 2 },
        },
    };

    private static string ScenePlan(string fontFile, int fontLength) => new JsonObject
    {
        ["scene"] = "level0", ["sprites"] = new JsonArray(), ["textures"] = new JsonArray(), ["fonts"] = new JsonArray(), ["axes"] = new JsonArray(),
        ["differences"] = new JsonArray(),
        ["assets"] = new JsonArray
        {
            Settings_("first", 1, 1), Settings_("second", 2, 0),
            new JsonObject { ["type"] = "Font", ["name"] = "Sans", ["file"] = fontFile, ["offset"] = 2, ["length"] = fontLength },
        },
        ["objects"] = new JsonArray
        {
            new JsonObject
            {
                ["name"] = "A", ["tag"] = "Untagged", ["layer"] = 0, ["active"] = true, ["parent"] = null, ["template"] = false,
                ["position"] = new JsonArray(0, 0, 0), ["rotation"] = new JsonArray(0, 0, 0, 1), ["scale"] = new JsonArray(1, 1, 1), ["rect"] = null,
                ["components"] = new JsonArray
                {
                    new JsonObject
                    {
                        ["type"] = "Script", ["class"] = "PlayerSceneAssetTests/Holder", ["assembly"] = typeof(PlayerSceneAssetTests).Assembly.GetName().Name,
                        ["enabled"] = true,
                        ["fields"] = new JsonObject { ["settings"] = new JsonObject { ["$asset"] = 0 }, ["again"] = new JsonObject { ["$asset"] = 0 } },
                    },
                },
            },
        },
    }.ToJsonString();
}
