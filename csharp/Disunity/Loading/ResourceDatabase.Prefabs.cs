using System.Text.Json;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Disunity.Hosting;

/// <summary>
/// Building the prefabs of a resource plan. A prefab's plan file is a scene plan of template
/// objects: they belong to no scene, stay inactive and wake only in their clones. Loading returns
/// the root game object. A component type asked of a prefab is answered by the root's component of
/// that type, or null when it has none, which is the engine's choice for how a game object resource
/// answers a component request.
/// </summary>
internal sealed partial class ResourceDatabase
{
    // Prefabs being built, so a cycle of prefabs that reference each other fails instead of recursing.
    private readonly HashSet<int> _prefabsBuilding = new();

    /// <summary>The object at the entry when it is a <paramref name="type"/>, or null.</summary>
    private Object? Fetch(Entry entry, Type type)
    {
        if (entry.Asset is { } index && IsPrefab(_assets[index]) && typeof(Component).IsAssignableFrom(type))
            return ((GameObject)Build(entry, typeof(GameObject))).GetComponent(type);
        return Matches(entry, type) ? Build(entry, type) : null;
    }

    private static bool IsPrefab(JsonElement asset) => asset.Str("type") is "Prefab" or "UnplannedPrefab";

    private GameObject Prefab(int index, JsonElement asset)
    {
        var name = asset.Str("name");
        if (!_prefabsBuilding.Add(index))
            throw new NotSupportedException($"Resources: prefab {name} references itself through other prefabs, which the engine does not load yet");
        try
        {
            using var plan = JsonDocument.Parse(File.ReadAllBytes(Path.Combine(_directory, asset.Str("plan"))));
            var options = new SceneLoadOptions(typeof(ResourceDatabase).Assembly,
                font => throw new NotSupportedException($"Resources: prefab {name} uses the package font {font}, which the engine does not load yet"))
            {
                Assemblies = _assemblies,
                ResourceAsset = Asset,
            };
            var loaded = new SceneLoader(plan.RootElement, options).LoadTemplates();
            DisunityHost.ResourceTemplates.UnionWith(loaded.TemplateObjects);
            return loaded.Objects[0];
        }
        finally { _prefabsBuilding.Remove(index); }
    }
}
