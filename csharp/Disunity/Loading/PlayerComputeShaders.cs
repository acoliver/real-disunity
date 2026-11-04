using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>Building a compute shader of a plan, for resource plans and scene plans alike: <c>{"type": "ComputeShader", "name",
/// "file", "path_id", "kernels": [{"name", "thread_group_size": [x, y, z] or null}]}</c>. The kernels are in index order. A size the
/// plan leaves null is one the host states when a script first needs it.</summary>
internal static class PlayerComputeShaders
{
    public static ComputeShader Build(JsonElement asset)
    {
        var kernels = asset.GetProperty("kernels").EnumerateArray().Select(kernel => (kernel.Str("name"), Size(kernel)));
        return DisunityHost.RegisterComputeShader(asset.Str("name"), asset.Str("file"), asset.GetProperty("path_id").GetInt64(), kernels.ToArray());
    }

    private static uint[]? Size(JsonElement kernel) =>
        kernel.TryGetProperty("thread_group_size", out var size) && size.ValueKind == JsonValueKind.Array
            ? size.EnumerateArray().Select(dimension => dimension.GetUInt32()).ToArray()
            : null;
}
