using System.Text.Json.Nodes;
using UnityEngine;
using UnityEngine.Experimental.Rendering;

/// <summary>Render textures a player's plans reference: built once from their plan with its size, formats, sample count, mip
/// settings and sampler, shared by every reference, loadable as a Texture or a RenderTexture, and failing by name when
/// they did not plan.</summary>
internal static class PlayerRenderTextureTests
{
    public sealed class Holder : ScriptableObject
    {
        public RenderTexture? first = null;
        public Texture? second = null;
        public RenderTexture? failed = null;
    }

    public static void Run() => PlanHarness.WithPlan(WritePlan, () =>
    {
        APlannedTextureCarriesItsValues();
        EveryReferenceMeetsOneObject();
        ATextureThatDidNotPlanFailsNamingTheCause();
    });

    private static JsonObject Target() => new()
    {
        ["type"] = "RenderTexture", ["name"] = "TimelineRenderTexture", ["width"] = 512, ["height"] = 256, ["depth_format"] = 90, ["color_format"] = 8,
        ["anti_aliasing"] = 4, ["mip_maps"] = true, ["generate_mips"] = false, ["dimension"] = 2, ["filter_mode"] = 2, ["aniso_level"] = 8,
        ["wrap_u"] = 0, ["wrap_v"] = 1, ["wrap_w"] = 2,
    };

    private static JsonObject Slot(string name, int texture) => new()
    {
        ["name"] = name, ["texture"] = texture, ["scale"] = new JsonArray(1f, 1f), ["offset"] = new JsonArray(0f, 0f),
    };

    private static string WritePlan(string directory)
    {
        File.WriteAllText(Path.Combine(directory, "holder.json"), """{"first":{"$asset":0},"second":{"$asset":0},"failed":null}""");
        var screen = new JsonObject
        {
            ["type"] = "Material", ["name"] = "Screen", ["shader"] = null, ["render_queue"] = -1, ["keywords"] = new JsonArray(), ["tags"] = new JsonArray(),
            ["disabled_passes"] = new JsonArray(), ["floats"] = new JsonArray(), ["ints"] = new JsonArray(), ["colors"] = new JsonArray(),
            ["textures"] = new JsonArray(Slot("_MainTex", 0)),
        };
        var assets = new JsonArray
        {
            Target(),
            new JsonObject { ["type"] = "UnplannedRenderTexture", ["error"] = "render texture object 3 of resources.assets: the string is truncated" },
            new JsonObject
            {
                ["type"] = "Script", ["class"] = "PlayerRenderTextureTests/Holder", ["assembly"] = typeof(PlayerRenderTextureTests).Assembly.GetName().Name,
                ["name"] = "holder", ["fields"] = "holder.json",
            },
            screen,
        };
        string[] paths = ["ui/materials/timelinerendertexture", "ui/materials/broken", "config/holder", "ui/materials/screen"];
        string[] classes = ["RenderTexture", "RenderTexture", "MonoBehaviour", "Material"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }

    private static void APlannedTextureCarriesItsValues()
    {
        var texture = Resources.Load<RenderTexture>("ui/materials/timelinerendertexture")!;
        Expect.Equal("TimelineRenderTexture", texture.name, "the texture has its planned name");
        Expect.That(texture.width == 512 && texture.height == 256, "the texture has its planned size");
        Expect.That(texture.graphicsFormat == GraphicsFormat.R8G8B8A8_UNorm && texture.depthStencilFormat == GraphicsFormat.D16_UNorm && texture.depth == 16,
            $"the texture has its planned formats, got {texture.graphicsFormat} {texture.depthStencilFormat} {texture.depth}");
        Expect.That(texture.antiAliasing == 4 && texture.useMipMap && !texture.autoGenerateMips && texture.dimension == UnityEngine.Rendering.TextureDimension.Tex2D,
            "the texture has its planned sample count, mip settings and dimension");
        Expect.That(texture.filterMode == FilterMode.Trilinear && texture.anisoLevel == 8, "the texture has its planned filter and anisotropy");
        Expect.That(texture.wrapModeU == TextureWrapMode.Repeat && texture.wrapModeV == TextureWrapMode.Clamp && texture.wrapModeW == TextureWrapMode.Mirror,
            "the texture has its planned wrap modes");
        Console.WriteLine("PASS: a planned render texture carries its name, size, formats, samples, mips and sampler");
    }

    private static void EveryReferenceMeetsOneObject()
    {
        var holder = Resources.Load<Holder>("config/holder")!;
        var loaded = Resources.Load<RenderTexture>("ui/materials/timelinerendertexture");
        Expect.That(loaded is not null && ReferenceEquals(holder.first, loaded) && ReferenceEquals(holder.second, loaded), "script fields name the one planned render texture");
        Expect.That(ReferenceEquals(Resources.Load<Texture>("ui/materials/timelinerendertexture"), loaded), "Resources.Load<Texture> returns the render texture");
        Expect.That(ReferenceEquals(Resources.Load<Material>("ui/materials/screen")!.GetTexture("_MainTex"), loaded), "a material slot holds the render texture");
        Expect.That(holder.failed is null, "an empty field stays null");
        Expect.That(Resources.Load<Texture2D>("ui/materials/timelinerendertexture") is null, "a render texture is not a Texture2D");
        Console.WriteLine("PASS: every reference to a planned render texture meets one object, loadable as a Texture or a RenderTexture");
    }

    private static void ATextureThatDidNotPlanFailsNamingTheCause()
    {
        var error = Expect.Catch<InvalidOperationException>(() => Resources.Load<Texture>("ui/materials/broken"), "loading a render texture that did not plan");
        Expect.That(error.Message.Contains("truncated") && error.Message.Contains("render texture object 3"), $"the failure names the cause, got {error.Message}");
        Console.WriteLine("PASS: loading a render texture that did not plan fails naming the cause");
    }
}
