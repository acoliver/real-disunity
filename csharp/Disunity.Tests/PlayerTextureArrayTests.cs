using System.Text.Json.Nodes;
using UnityEngine;
using UnityEngine.Experimental.Rendering;
using UnityEngine.Rendering;
using static GraphicsTestSupport;

/// <summary>Texture arrays a player's plans reference: built once from their plan with size, slice count, format, bytes and
/// sampler, shared by every reference, kept as bytes when the format has no codec, and failing by name when the plan
/// disagrees with the format.</summary>
internal static class PlayerTextureArrayTests
{
    public sealed class Holder : ScriptableObject
    {
        public Texture2DArray? first = null;
        public Texture? second = null;
        public Texture2DArray? packed = null;
    }

    public static void Run() => PlanHarness.WithPlan(WritePlan, () =>
    {
        APlannedArrayCarriesItsValuesAndBytes();
        EveryReferenceMeetsOneObject();
        AFormatWithoutAMappingKeepsItsBytes();
        APlanThatDisagreesWithItsFormatFailsByName();
    });

    /// <summary>A 2x1 array of three slices in R8G8B8A8_UNorm: slice s, texel t has red 10*s + t.</summary>
    private static byte[] Pixels()
    {
        var bytes = new byte[2 * 1 * 3 * 4];
        for (var slice = 0; slice < 3; slice++)
            for (var texel = 0; texel < 2; texel++)
                bytes[(slice * 2 + texel) * 4] = (byte)(10 * slice + texel);
        return bytes;
    }

    private static JsonObject Planned(string name, int format, long size, string file) => new()
    {
        ["type"] = "Texture2DArray", ["name"] = name, ["width"] = 2, ["height"] = 1, ["depth"] = 3, ["graphics_format"] = format, ["mip_count"] = 1,
        ["readable"] = true, ["color_space"] = 0, ["filter_mode"] = 2, ["aniso_level"] = 8, ["mip_bias"] = 0.5, ["wrap_u"] = 0, ["wrap_v"] = 1, ["wrap_w"] = 2,
        ["data"] = new JsonObject { ["file"] = file, ["offset"] = 4, ["size"] = size },
    };

    private static string WritePlan(string directory)
    {
        var data = new byte[4].Concat(Pixels()).ToArray();
        File.WriteAllBytes(Path.Combine(directory, "atlas.bin"), data);
        File.WriteAllBytes(Path.Combine(directory, "raw.bin"), new byte[4 + 40]);
        File.WriteAllText(Path.Combine(directory, "holder.json"), """{"first":{"$asset":0},"second":{"$asset":0},"packed":{"$asset":1}}""");
        var assets = new JsonArray
        {
            Planned("Atlas", (int)GraphicsFormat.R8G8B8A8_UNorm, 24, "atlas.bin"),
            Planned("Packed", 87, 40, "raw.bin"),
            Planned("Short", (int)GraphicsFormat.R8G8B8A8_UNorm, 23, "atlas.bin"),
            new JsonObject
            {
                ["type"] = "Script", ["class"] = "PlayerTextureArrayTests/Holder", ["assembly"] = typeof(PlayerTextureArrayTests).Assembly.GetName().Name,
                ["name"] = "holder", ["fields"] = "holder.json",
            },
        };
        string[] paths = ["atlas", "packed", "short", "config/holder"];
        string[] classes = ["Texture2DArray", "Texture2DArray", "Texture2DArray", "MonoBehaviour"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static void APlannedArrayCarriesItsValuesAndBytes()
    {
        var array = Resources.Load<Texture2DArray>("atlas")!;
        Expect.Equal("Atlas", array.name, "the array has its planned name");
        Expect.That(array.width == 2 && array.height == 1 && array.depth == 3, "the array has its planned size and slice count");
        Expect.That(array.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm && array.format == TextureFormat.RGBA32 && array.mipmapCount == 1,
            $"the array has its planned format, got {array.graphicsFormat}");
        Expect.That(array.isReadable, "the array is readable as planned");
        Expect.That(array.filterMode == FilterMode.Trilinear && array.anisoLevel == 8 && array.mipMapBias == 0.5f, "the array has its planned filter, anisotropy and bias");
        Expect.That(array.wrapModeU == TextureWrapMode.Repeat && array.wrapModeV == TextureWrapMode.Clamp && array.wrapModeW == TextureWrapMode.Mirror,
            "the array has its planned wrap modes");
        for (var slice = 0; slice < 3; slice++)
            for (var texel = 0; texel < 2; texel++)
                Expect.Near((10 * slice + texel) / 255f, array.GetPixels(slice)[texel].r, "slice " + slice + " texel " + texel + " has the bytes of the file");
        Console.WriteLine("PASS: a planned texture array carries its name, size, slices, format, sampler and bytes");
    }

    private static void EveryReferenceMeetsOneObject()
    {
        var holder = Resources.Load<Holder>("config/holder")!;
        var loaded = Resources.Load<Texture2DArray>("atlas");
        Expect.That(loaded is not null && ReferenceEquals(holder.first, loaded) && ReferenceEquals(holder.second, loaded), "script fields name the one planned array");
        Expect.That(ReferenceEquals(Resources.Load<Texture>("atlas"), loaded), "Resources.Load<Texture> returns the array");
        Expect.That(Resources.Load<Texture2D>("atlas") is null, "an array is not a Texture2D");
        Console.WriteLine("PASS: every reference to a planned texture array meets one object");
    }

    private static void AFormatWithoutAMappingKeepsItsBytes()
    {
        var packed = Resources.Load<Texture2DArray>("packed")!;
        Expect.That(packed.depth == 3 && packed.graphicsFormat == (GraphicsFormat)87, "an unmapped format array keeps its size and GraphicsFormat");
        Throws<NotSupportedException>(() => _ = packed.format, "the TextureFormat of an unmapped format", "Texture2DArray.format");
        NotSupported(() => packed.GetPixels(0), "reading an unmapped format", "Texture2DArray.GetPixels");
        Expect.That(ReferenceEquals(Resources.Load<Holder>("config/holder")!.packed, packed), "a script field names the unmapped array");
        Console.WriteLine("PASS: an array in a format without a TextureFormat keeps its bytes and refuses pixel access");
    }

    private static void APlanThatDisagreesWithItsFormatFailsByName()
    {
        var error = Expect.Catch<InvalidDataException>(() => Resources.Load<Texture2DArray>("short"), "loading an array whose plan holds too few bytes");
        Expect.That(error.Message.Contains("Short") && error.Message.Contains("23") && error.Message.Contains("24"), $"the failure names the array and sizes, got {error.Message}");
        Console.WriteLine("PASS: an array whose plan disagrees with its format fails naming it");
    }
}
