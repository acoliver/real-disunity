using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building a shader of a plan, for resource plans and scene plans alike: <c>{"type": "Shader", "name", "render_queue",
/// "pass_count", "subshader_count"}</c>. The shader is the one registered under its name, so every reference to it, and
/// <see cref="Shader.Find"/>, meet the same object. Its programs are not planned.</summary>
internal static class PlayerShaders
{
    public static Shader Build(JsonElement asset, Action<string> difference) =>
        DisunityHost.DeclareShader(asset.Str("name"), asset.Int("render_queue"), asset.Int("pass_count"), asset.Int("subshader_count"), difference);
}
