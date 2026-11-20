using System.Text.Json.Nodes;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;

/// <summary>Cubemaps and 3D textures a player's plans reference: built once from their plan with size, format, bytes and
/// sampler, shared by every reference, and failing by name when the plan disagrees with the format.</summary>
internal static class PlayerLayeredTextureTests
{
    public sealed class Holder : ScriptableObject
    {
        public Cubemap? sky = null;
        public Texture? skyAgain = null;
        public Texture3D? noise = null;
        public Texture? noiseAgain = null;
    }

    public static void Run() => PlanHarness.WithPlan(WritePlan, () =>
    {
        APlannedCubemapCarriesItsValuesAndBytes();
        APlannedVolumeCarriesItsValuesAndBytes();
        EveryReferenceMeetsOneObject();
        APlanThatDisagreesWithItsFormatFailsByName();
    });

    /// <summary>The file holds four padding bytes, then bytes counting up from zero, so a byte's value is its offset in the pixels.</summary>
    private static byte[] Counting(int length) => new byte[4].Concat(Enumerable.Range(0, length).Select(i => (byte)i)).ToArray();

    private static JsonObject Sampler(JsonObject asset, int colorSpace = 0)
    {
        asset["readable"] = true;
        asset["color_space"] = colorSpace;
        asset["filter_mode"] = 2;
        asset["aniso_level"] = 8;
        asset["mip_bias"] = 0.5;
        asset["wrap_u"] = 0;
        asset["wrap_v"] = 1;
        asset["wrap_w"] = 2;
        return asset;
    }

    private static JsonObject Data(string file, long size) => new() { ["file"] = file, ["offset"] = 4, ["size"] = size };

    /// <summary>Six 2x2 RGBA32 faces with two levels, 20 bytes a face, in sRGB (color space 1).</summary>
    private static JsonObject Cube(string name, long size) => Sampler(new JsonObject
    {
        ["type"] = "Cubemap", ["name"] = name, ["width"] = 2, ["height"] = 2, ["format"] = (int)TextureFormat.RGBA32, ["mip_count"] = 2,
        ["data"] = Data("sky.bin", size),
    }, colorSpace: 1);

    /// <summary>A 2x1x4 volume in R8G8B8A8_SRGB with three levels: 32, 8 and 4 bytes.</summary>
    private static JsonObject Volume(string name, long size) => Sampler(new JsonObject
    {
        ["type"] = "Texture3D", ["name"] = name, ["width"] = 2, ["height"] = 1, ["depth"] = 4, ["graphics_format"] = (int)GraphicsFormat.R8G8B8A8_SRGB,
        ["mip_count"] = 3, ["data"] = Data("noise.bin", size),
    });

    private static string WritePlan(string directory)
    {
        File.WriteAllBytes(Path.Combine(directory, "sky.bin"), Counting(120));
        File.WriteAllBytes(Path.Combine(directory, "noise.bin"), Counting(44));
        File.WriteAllText(Path.Combine(directory, "holder.json"), """{"sky":{"$asset":0},"skyAgain":{"$asset":0},"noise":{"$asset":1},"noiseAgain":{"$asset":1}}""");
        var assets = new JsonArray
        {
            Cube("Sky", 120), Volume("Noise", 44), Cube("ShortSky", 119), Volume("ShortNoise", 43),
            new JsonObject
            {
                ["type"] = "Script", ["class"] = "PlayerLayeredTextureTests/Holder", ["assembly"] = typeof(PlayerLayeredTextureTests).Assembly.GetName().Name,
                ["name"] = "holder", ["fields"] = "holder.json",
            },
        };
        string[] paths = ["sky", "noise", "shortsky", "shortnoise", "config/holder"];
        string[] classes = ["Cubemap", "Texture3D", "Cubemap", "Texture3D", "MonoBehaviour"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static void ExpectSampler(Texture texture, string what)
    {
        Expect.That(texture.filterMode == FilterMode.Trilinear && texture.anisoLevel == 8 && texture.mipMapBias == 0.5f, $"the {what} has its planned filter, anisotropy and bias");
        Expect.That(texture.wrapModeU == TextureWrapMode.Repeat && texture.wrapModeV == TextureWrapMode.Clamp && texture.wrapModeW == TextureWrapMode.Mirror,
            $"the {what} has its planned wrap modes");
    }

    private static void APlannedCubemapCarriesItsValuesAndBytes()
    {
        var cube = Resources.Load<Cubemap>("sky")!;
        Expect.Equal("Sky", cube.name, "the cubemap has its planned name");
        Expect.That(cube.width == 2 && cube.height == 2 && cube.mipmapCount == 2 && cube.dimension == TextureDimension.Cube, "the cubemap has its planned size, levels and dimension");
        Expect.That(cube.format == TextureFormat.RGBA32 && cube.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB, $"the cubemap has its planned format in sRGB, got {cube.graphicsFormat}");
        Expect.Equal(120, cube.ByteLength, "the cubemap holds six faces of two levels");
        Expect.That(cube.PixelBytes[0] == 0 && cube.PixelBytes[20] == 20 && cube.PixelBytes[119] == 119, "the cubemap holds the bytes of the file after its offset");
        ExpectSampler(cube, "cubemap");
        Console.WriteLine("PASS: a planned cubemap carries its name, size, format, sampler and bytes");
    }

    private static void APlannedVolumeCarriesItsValuesAndBytes()
    {
        var volume = Resources.Load<Texture3D>("noise")!;
        Expect.Equal("Noise", volume.name, "the volume has its planned name");
        Expect.That(volume.width == 2 && volume.height == 1 && volume.depth == 4 && volume.mipmapCount == 3, "the volume has its planned size, depth and levels");
        Expect.That(volume.graphicsFormat == GraphicsFormat.R8G8B8A8_SRGB && volume.isReadable, $"the volume has its planned format and readable flag, got {volume.graphicsFormat}");
        Expect.Equal(44, volume.ByteLength, "the volume holds every level with its slices");
        Expect.That(volume.PixelBytes[0] == 0 && volume.PixelBytes[32] == 32 && volume.PixelBytes[43] == 43, "the volume holds the bytes of the file after its offset");
        ExpectSampler(volume, "volume");
        Console.WriteLine("PASS: a planned 3D texture carries its name, size, depth, format, sampler and bytes");
    }

    private static void EveryReferenceMeetsOneObject()
    {
        var holder = Resources.Load<Holder>("config/holder")!;
        var (cube, volume) = (Resources.Load<Cubemap>("sky"), Resources.Load<Texture3D>("noise"));
        Expect.That(cube is not null && ReferenceEquals(holder.sky, cube) && ReferenceEquals(holder.skyAgain, cube), "script fields name the one planned cubemap");
        Expect.That(volume is not null && ReferenceEquals(holder.noise, volume) && ReferenceEquals(holder.noiseAgain, volume), "script fields name the one planned volume");
        Expect.That(ReferenceEquals(Resources.Load<Texture>("sky"), cube) && ReferenceEquals(Resources.Load<Texture>("noise"), volume), "Resources.Load<Texture> returns them");
        Expect.That(Resources.Load<Texture2D>("sky") is null && Resources.Load<Texture2D>("noise") is null, "neither is a Texture2D");
        Expect.That(Resources.Load<Cubemap>("noise") is null && Resources.Load<Texture3D>("sky") is null, "a cubemap is not a volume");
        Console.WriteLine("PASS: every reference to a planned cubemap or 3D texture meets one object");
    }

    private static void APlanThatDisagreesWithItsFormatFailsByName()
    {
        var cube = Expect.Catch<InvalidDataException>(() => Resources.Load<Cubemap>("shortsky"), "loading a cubemap whose plan holds too few bytes");
        Expect.That(cube.Message.Contains("ShortSky") && cube.Message.Contains("119") && cube.Message.Contains("120"), $"the failure names the cubemap and sizes, got {cube.Message}");
        var volume = Expect.Catch<InvalidDataException>(() => Resources.Load<Texture3D>("shortnoise"), "loading a volume whose plan holds too few bytes");
        Expect.That(volume.Message.Contains("ShortNoise") && volume.Message.Contains("43") && volume.Message.Contains("44"), $"the failure names the volume and sizes, got {volume.Message}");
        Console.WriteLine("PASS: a cubemap or 3D texture whose plan disagrees with its format fails naming it");
    }
}
