using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building a material of a plan, for resource plans and scene plans alike: <c>{"type": "Material", "name",
/// "shader" (asset index or null), "render_queue" (-1 for the shader's), "keywords", "tags" [[key, value]],
/// "disabled_passes", "floats" [[name, value]], "ints", "colors" [[name, [r, g, b, a]]], "vectors" [[name, [x, y, z, w]]] (optional), "textures" [{"name", "texture"
/// (asset index or null), "unsent" (true for a texture the plan did not build; it is not a null slot), "scale", "offset"}]}</c>. The material is <c>new Material(shader)</c> with the planned values
/// set through the public setters: a color as authored (gamma), which <see cref="Material.SetColor(string, Color)"/> converts to linear in a
/// linear color space, and a vector as it is. A material stores both in one list; the plan carries the properties its shader declares as
/// vectors apart, and a property the shader does not declare as a vector is a color. A material without a shader (none, or one that did not plan, which the plan lists
/// as a difference) gets the shader named <c>Hidden/InternalErrorShader</c> (the engine's choice of fallback: queue 2000, one pass, no tags and no fallback shader of its own). Nothing
/// draws with the material yet.</summary>
internal static class PlayerMaterials
{
    private const string FallbackShader = "Hidden/InternalErrorShader";
    private const int FallbackQueue = 2000;

    /// <summary>Builds the material. <paramref name="resolve"/> gives the built object of a planned asset index.</summary>
    public static Material Build(JsonElement plan, Func<int, object> resolve)
    {
        var shader = plan.GetProperty("shader") is { ValueKind: JsonValueKind.Number } index
            ? (Shader)resolve(index.GetInt32())
            : DisunityHost.DeclareShader(FallbackShader, new ShaderDeclaration(FallbackQueue, 1, 1, [""], [], "", DeclarationOrigin.Engine), _ => { });
        var material = new Material(shader) { name = plan.Str("name"), renderQueue = plan.Int("render_queue") };
        foreach (var keyword in plan.GetProperty("keywords").EnumerateArray()) material.EnableKeyword(keyword.GetString()!);
        foreach (var tag in plan.GetProperty("tags").EnumerateArray()) material.SetOverrideTag(tag[0].GetString()!, tag[1].GetString()!);
        foreach (var pass in plan.GetProperty("disabled_passes").EnumerateArray()) material.SetShaderPassEnabled(pass.GetString()!, false);
        SetProperties(material, plan);
        foreach (var slot in plan.GetProperty("textures").EnumerateArray()) SetTexture(material, slot, resolve);
        return material;
    }

    private static void SetProperties(Material material, JsonElement plan)
    {
        foreach (var pair in plan.GetProperty("floats").EnumerateArray()) material.SetFloat(pair[0].GetString()!, pair[1].GetSingle());
        foreach (var pair in plan.GetProperty("ints").EnumerateArray()) material.SetInt(pair[0].GetString()!, pair[1].GetInt32());
        foreach (var pair in plan.GetProperty("colors").EnumerateArray())
        {
            var c = pair[1];
            material.SetColor(pair[0].GetString()!, new Color(c[0].GetSingle(), c[1].GetSingle(), c[2].GetSingle(), c[3].GetSingle()));
        }
        if (!plan.TryGetProperty("vectors", out var vectors)) return;
        foreach (var pair in vectors.EnumerateArray())
        {
            var v = pair[1];
            material.SetVector(pair[0].GetString()!, new Vector4(v[0].GetSingle(), v[1].GetSingle(), v[2].GetSingle(), v[3].GetSingle()));
        }
    }

    /// <summary>The texture of a slot that names a texture the plan did not build: an empty <see cref="Texture2D"/>, which the world
    /// is sent as named and not sent (see <c>WorldGraphics.TextureOf</c>), so the slot is not the game's own null.</summary>
    private static Texture2D UnbuiltTexture(string slot) => new(0, 0) { name = $"unbuilt texture of {slot}" };

    private static void SetTexture(Material material, JsonElement slot, Func<int, object> resolve)
    {
        var name = slot.Str("name");
        var texture = slot.GetProperty("texture") is { ValueKind: JsonValueKind.Number } index ? (Texture)resolve(index.GetInt32())
            : slot.TryGetProperty("unsent", out var unsent) && unsent.ValueKind == JsonValueKind.True ? UnbuiltTexture(name)
            : null;
        material.SetTexture(name, texture!);
        material.SetTextureScale(name, slot.Vec2("scale"));
        material.SetTextureOffset(name, slot.Vec2("offset"));
    }
}
