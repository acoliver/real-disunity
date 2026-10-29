using System.Text.Json.Nodes;
using Disunity.Hosting;

/// <summary>Loads script assets through a resource plan, the way a player's resources load, so a
/// test observes how the field binder fills their serialized fields from plan JSON.</summary>
internal static class PlanHarness
{
    /// <summary>A script asset of <paramref name="Type"/> named <paramref name="Name"/>, bound from <paramref name="Fields"/> (JSON).</summary>
    public readonly record struct Asset(Type Type, string Name, string Fields);

    /// <summary>The path an asset loads from.</summary>
    public static string PathOf(Asset asset) => $"data/{asset.Name}";

    public static void With(IReadOnlyList<Asset> assets, Action run)
    {
        var directory = Directory.CreateTempSubdirectory("disunity-plan-");
        try
        {
            DisunityHost.LoadResources(WritePlan(directory.FullName, assets), _ => typeof(PlanHarness).Assembly);
            DisunityHost.LoadWorld();
            try { run(); }
            finally { DisunityHost.UnloadWorld(); }
        }
        finally
        {
            DisunityHost.UnloadResources();
            directory.Delete(recursive: true);
        }
    }

    private static string WritePlan(string directory, IReadOnlyList<Asset> assets)
    {
        var entries = new JsonArray();
        var planned = new JsonArray();
        for (var i = 0; i < assets.Count; i++)
        {
            var asset = assets[i];
            File.WriteAllText(Path.Combine(directory, $"{i}.json"), asset.Fields);
            entries.Add(new JsonObject { ["path"] = PathOf(asset), ["class"] = "MonoBehaviour", ["asset"] = i });
            planned.Add(new JsonObject
            {
                ["type"] = "Script", ["class"] = asset.Type.FullName!.Replace('+', '/'),
                ["assembly"] = asset.Type.Assembly.GetName().Name, ["name"] = asset.Name, ["fields"] = $"{i}.json",
            });
        }
        var plan = new JsonObject { ["entries"] = entries, ["assets"] = planned, ["differences"] = new JsonArray() };
        var path = Path.Combine(directory, "resources.json");
        File.WriteAllText(path, plan.ToJsonString());
        return path;
    }
}
