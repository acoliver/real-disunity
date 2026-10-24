using System.Text.Json;
using System.Text.Json.Nodes;
using CompatibilityBridge;
using UnityEngine;

internal static class GraphFixture
{
    public static JsonDocument Payload(string repository, string game, string graphName, string? selectedProfile = null)
    {
        using var profiles = JsonDocument.Parse(File.ReadAllText(Path.Combine(repository, "csharp/Compatibility.Tests/OriginalGames/profiles.json")));
        var source = profiles.RootElement.GetProperty("sources").GetProperty(game);
        var profileId = selectedProfile ?? source.GetProperty("default_profile").GetString()!;
        var profile = profiles.RootElement.GetProperty("profiles").GetProperty(profileId);
        using var graph = JsonDocument.Parse(File.ReadAllText(Path.Combine(repository, ".work/content", graphName + ".json")));
        var root = graph.RootElement;
        var handles = CreateObjects(root);
        CreateComponents(root, handles);
        foreach (var component in root.GetProperty("components").EnumerateArray())
        {
            var id = component.GetProperty("id").GetString()!;
            if (!handles.TryGetValue(id, out var handle) || component.GetProperty("native_kind").GetString() == "Transform") continue;
            NativeWorld.Request("component.set", ("component", handle), ("enabled", component.GetProperty("enabled").GetBoolean()),
                ("fields", Rewrite(component.GetProperty("native_fields"), handles))).Dispose();
        }
        return JsonDocument.Parse(JsonSerializer.Serialize(new
        {
            assemblies = new[] { Path.Combine(Environment.GetEnvironmentVariable("DISUNITY_TEST_MANAGED_ROOT") ?? Path.Combine(repository, ".work/managed"), "artifacts/bin", game, "debug", game + ".dll") },
            graph = root, handles,
            profile = new
            {
                id = profileId, unknown_fields = profile.GetProperty("unknown_fields"),
                field_aliases = source.GetProperty("field_aliases")
            }
        }));
    }
    private static Dictionary<string, ulong> CreateObjects(JsonElement graph)
    {
        var handles = new Dictionary<string, ulong>();
        foreach (var node in graph.GetProperty("nodes").EnumerateArray())
        {
            using var created = NativeWorld.Request("object.create", ("name", node.GetProperty("name").GetString()), ("tag", node.GetProperty("tag").GetString()),
                ("active", node.GetProperty("active").GetBoolean() && !node.GetProperty("template").GetBoolean()));
            var handle = NativeWorld.Data(created).GetProperty("handle").GetUInt64();
            handles.Add(node.GetProperty("id").GetString()!, handle);
            var trs = node.GetProperty("transform");
            NativeWorld.Request("transform.set", ("object", handle), ("position", trs.GetProperty("local_position")),
                ("rotation", trs.GetProperty("local_rotation")), ("scale", trs.GetProperty("local_scale"))).Dispose();
        }
        foreach (var node in graph.GetProperty("nodes").EnumerateArray())
            if (node.GetProperty("parent").ValueKind != JsonValueKind.Null)
                NativeWorld.Request("transform.parent", ("object", handles[node.GetProperty("id").GetString()!]),
                    ("parent", handles[node.GetProperty("parent").GetString()!]), ("world_position_stays", false)).Dispose();
        return handles;
    }
    private static void CreateComponents(JsonElement graph, Dictionary<string, ulong> handles)
    {
        foreach (var component in graph.GetProperty("components").EnumerateArray())
        {
            if (component.GetProperty("object").ValueKind == JsonValueKind.Null) continue;
            var kind = component.GetProperty("native_kind").GetString();
            if (kind is null) continue;
            var owner = handles[component.GetProperty("object").GetString()!];
            using var created = kind == "Transform" ? NativeWorld.Request("component.list", ("object", owner), ("kind", "Transform"))
                : NativeWorld.Request("component.add", ("object", owner), ("kind", kind));
            var data = NativeWorld.Data(created);
            handles.Add(component.GetProperty("id").GetString()!, (kind == "Transform" ? data[0] : data).GetProperty("handle").GetUInt64());
        }
    }
    private static JsonNode? Rewrite(JsonElement value, Dictionary<string, ulong> handles)
    {
        if (value.ValueKind == JsonValueKind.Object)
        {
            var result = new JsonObject();
            foreach (var property in value.EnumerateObject())
                result[property.Name] = property.Name == "$ref" && property.Value.ValueKind == JsonValueKind.String && handles.TryGetValue(property.Value.GetString()!, out var handle)
                    ? JsonValue.Create(handle) : Rewrite(property.Value, handles);
            return result;
        }
        if (value.ValueKind == JsonValueKind.Array) return new JsonArray(value.EnumerateArray().Select(item => Rewrite(item, handles)).ToArray());
        return JsonNode.Parse(value.GetRawText());
    }
}
