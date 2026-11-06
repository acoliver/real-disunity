using System.Text.Json.Nodes;
using Disunity.Hosting;
using UnityEngine;

/// <summary>Compute shaders a player's plans reference: a script field and a Resources path resolve to the planned asset, which has the
/// kernels and sizes the plan states, and one that did not plan binds null.</summary>
internal static class PlayerComputeShaderTests
{
    public sealed class Holder : ScriptableObject
    {
        public ComputeShader? readBack = null;
        public ComputeShader? failed = null;
    }

    public static void Run()
    {
        PlanHarness.WithPlan(WritePlan, () =>
        {
            var holder = Resources.Load<Holder>("config/holder")!;
            Expect.That(holder.readBack is not null, "a serialized ComputeShader reference resolves to the planned asset, not null");
            Expect.That(holder.failed is null, "a reference to a compute shader that did not plan binds null");
            Expect.Equal("TerrainReadBackShader", holder.readBack!.name, "the planned name");
            Expect.That(holder.readBack.FindKernel("TerrainReadBackCS") == 0, "the planned kernel");
            holder.readBack.GetKernelThreadGroupSizes(0, out var x, out var y, out var z);
            Expect.That((x, y, z) == (8u, 8u, 1u), "the planned thread group size");
            Expect.That(ReferenceEquals(Resources.Load<ComputeShader>("compute/read"), holder.readBack), "Resources.Load returns the same asset");
            Expect.That(Resources.Load<Texture2D>("compute/read") is null, "a compute shader is not any other type");
            var error = Expect.Catch<InvalidOperationException>(() => Resources.Load<ComputeShader>("compute/broken"), "loading a compute shader that did not plan");
            Expect.That(error.Message.Contains("truncated"), "the failure names the cause: " + error.Message);
        });
        Console.WriteLine("PASS: a planned compute shader resolves from a script field and from Resources, with its kernels and sizes, and one that did not plan binds null");
    }

    private static string WritePlan(string directory)
    {
        File.WriteAllText(Path.Combine(directory, "holder.json"), """{"readBack":{"$asset":0},"failed":null}""");
        var assets = new JsonArray
        {
            JsonNode.Parse(ComputeTestSupport.TerrainPlan)!,
            new JsonObject { ["type"] = "UnplannedComputeShader", ["error"] = "compute shader object 9 of resources.assets: the data is truncated" },
            new JsonObject
            {
                ["type"] = "Script", ["class"] = "PlayerComputeShaderTests/Holder", ["assembly"] = typeof(PlayerComputeShaderTests).Assembly.GetName().Name,
                ["name"] = "holder", ["fields"] = "holder.json",
            },
        };
        string[] paths = ["compute/read", "compute/broken", "config/holder"];
        string[] classes = ["ComputeShader", "ComputeShader", "MonoBehaviour"];
        var entries = new JsonArray(paths.Select((path, i) => (JsonNode)new JsonObject { ["path"] = path, ["class"] = classes[i], ["asset"] = i }).ToArray());
        return PlanHarness.WriteJson(directory, entries, assets);
    }
}
