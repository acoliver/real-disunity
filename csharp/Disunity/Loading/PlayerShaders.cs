using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building a shader of a plan, for resource plans and scene plans alike: <c>{"type": "Shader", "name", "render_queue",
/// "pass_count", "pass_names", "subshader_count", "tags" [[key, value]], "fallback", "file", "path_id"}</c>, where
/// <c>tags</c> are the first sub-shader's and <c>fallback</c> is the fallback shader's name, empty for none. The shader is the one registered under its name, so every reference to it, and
/// <see cref="Shader.Find"/>, meet the same object. Its programs are not planned; <c>file</c> and <c>path_id</c> name the serialized
/// shader object a host reads them from.</summary>
internal static class PlayerShaders
{
    public static Shader Build(JsonElement asset, Action<string> difference) =>
        DisunityHost.DeclareShader(asset.Str("name"), Declaration(asset), difference,
            asset.TryGetProperty("file", out var file) ? file.GetString() : null, asset.TryGetProperty("path_id", out var pathId) ? pathId.GetInt64() : 0);

    private static ShaderDeclaration Declaration(JsonElement asset) => new(asset.Int("render_queue"), asset.Int("pass_count"), asset.Int("subshader_count"), PassNames(asset),
        asset.GetProperty("tags").EnumerateArray().Select(tag => KeyValuePair.Create(tag[0].GetString() ?? "", tag[1].GetString() ?? "")).ToArray(), asset.Str("fallback"));

    /// <summary>The names of the first sub-shader's passes in order, as many as <c>pass_count</c> says; a plan that disagrees with
    /// itself is refused.</summary>
    private static string[] PassNames(JsonElement asset)
    {
        var names = asset.GetProperty("pass_names").EnumerateArray().Select(name => name.GetString() ?? "").ToArray();
        var count = asset.Int("pass_count");
        return names.Length == count ? names : throw new InvalidOperationException($"shader '{asset.Str("name")}' declares {count} passes but names {names.Length}");
    }
}
