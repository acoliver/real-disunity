using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building a material of a plan, for resource plans and scene plans alike: <c>{"type": "Material", "name",
/// "shader" (asset index or null), "render_queue" (-1 for the shader's), "keywords", "tags" [[key, value]],
/// "disabled_passes", "floats" [[name, value]], "ints", "colors" [[name, [r, g, b, a]]], "textures" [{"name", "texture"
/// (asset index or null), "scale", "offset"}]}</c>. The material is <c>new Material(shader)</c> with the planned values
/// set through the public setters. A material without a shader (none, or one that did not plan, which the plan lists
/// as a difference) gets the shader named <c>Hidden/InternalErrorShader</c> (the engine's choice of fallback: queue 2000, one pass). Nothing
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
            : DisunityHost.DeclareShader(FallbackShader, FallbackQueue, 1, 1, _ => { });
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
    }

    private static void SetTexture(Material material, JsonElement slot, Func<int, object> resolve)
    {
        var name = slot.Str("name");
        var texture = slot.GetProperty("texture") is { ValueKind: JsonValueKind.Number } index ? (Texture)resolve(index.GetInt32()) : null;
        material.SetTexture(name, texture!);
        material.SetTextureScale(name, slot.Vec2("scale"));
        material.SetTextureOffset(name, slot.Vec2("offset"));
    }
}
