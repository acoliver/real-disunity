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

    public static void With(IReadOnlyList<Asset> assets, Action run) =>
        WithPlan(directory => WritePlan(directory, assets), run);

    /// <summary>Loads the plan whose path <paramref name="write"/> returns after writing it into a fresh directory.</summary>
    public static void WithPlan(Func<string, string> write, Action run)
    {
        var directory = Directory.CreateTempSubdirectory("disunity-plan-");
        try
        {
            DisunityHost.LoadResources(write(directory.FullName), _ => typeof(PlanHarness).Assembly);
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

    /// <summary>Writes <paramref name="entries"/> and <paramref name="assets"/> as a plan in <paramref name="directory"/>.</summary>
    public static string WriteJson(string directory, JsonArray entries, JsonArray assets)
    {
        var plan = new JsonObject { ["entries"] = entries, ["assets"] = assets, ["differences"] = new JsonArray() };
        var path = Path.Combine(directory, "resources.json");
        File.WriteAllText(path, plan.ToJsonString());
        return path;
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
        return WriteJson(directory, entries, planned);
    }
}
