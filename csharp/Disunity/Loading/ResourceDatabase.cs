using System.Reflection;
using System.Text.Json;
using UnityEngine;
using Object = UnityEngine.Object;

namespace Disunity.Hosting;

/// <summary>
/// A player's resources from a plan <c>unity-import</c> writes (<c>player_resources</c>).
/// Each container entry names a lower-case path, the Unity class of its object and, when the
/// engine builds that class, a planned asset. Assets are built the first time they load and
/// kept for the world's lifetime: a text asset reads its byte range from the player file, a
/// texture, sprite or sprite atlas is built from its plan (see <c>ResourceDatabase.Sprites.cs</c>), and
/// a script asset binds its fields before its <c>Awake</c> and <c>OnEnable</c>, as Unity
/// deserializes an asset before activating it. Script assets one load reaches through their
/// references are all bound before any of them is activated, so assets that reference each
/// other see each other's data; when one fails, none of them is kept. A prefab is built from its
/// own plan into template objects (see <c>ResourceDatabase.Prefabs.cs</c>). A destroyed asset is
/// built again on its next load. Loading an entry of a class the engine does not build fails
/// with the path and class named. Resources load on the main thread only.
/// </summary>
internal sealed partial class ResourceDatabase
{
    private readonly record struct Entry(string Path, string Class, int? Asset);

    private readonly List<Entry> _entries = new();
    private readonly List<string> _differences = new();
    private readonly List<JsonElement> _assets = new();
    private readonly string _directory;
    // The plan's graphics section, absent from plans of projects without player settings.
    private readonly JsonElement? _graphics;
    private readonly Func<string, Assembly> _assemblies;
    private readonly Dictionary<int, Object> _built = new();
    // Script assets created by the load in progress, in creation order, waiting to activate.
    private readonly List<(int Index, ScriptableObject Asset)> _binding = new();

    public ResourceDatabase(string planPath, Func<string, Assembly> assemblies)
    {
        _directory = Path.GetDirectoryName(Path.GetFullPath(planPath))!;
        _assemblies = assemblies;
        using var plan = JsonDocument.Parse(File.ReadAllBytes(planPath));
        foreach (var entry in plan.RootElement.GetProperty("entries").EnumerateArray())
        {
            var asset = entry.GetProperty("asset");
            _entries.Add(new Entry(Normalize(entry.Str("path")), entry.Str("class"),
                asset.ValueKind == JsonValueKind.Null ? null : asset.GetInt32()));
        }
        foreach (var asset in plan.RootElement.GetProperty("assets").EnumerateArray()) _assets.Add(asset.Clone());
        if (plan.RootElement.TryGetProperty("graphics", out var graphics)) _graphics = graphics.Clone();
    }

    /// <summary>What the engine left out of the resources it built, one line each. Each is also logged as a
    /// warning when it is recorded, so a player run reports it.</summary>
    public IReadOnlyList<string> Differences => _differences;

    private void Difference(string line)
    {
        _differences.Add(line);
        Debug.LogWarning($"not reproduced: {line}");
    }

    /// <summary>Forgets the built assets, which belong to the world being unloaded.</summary>
    public void Clear() => _built.Clear();

    /// <summary>The first object at <paramref name="path"/> that is a <paramref name="type"/>, or null.</summary>
    public Object? Load(string path, Type type)
    {
        RequireMainThread();
        var key = Normalize(path);
        foreach (var entry in _entries)
            if (entry.Path == key && Fetch(entry, type) is { } found) return found;
        return null;
    }

    /// <summary>Every object of <paramref name="type"/> at <paramref name="path"/> or in the folder it names.</summary>
    public Object[] LoadAll(string path, Type type)
    {
        RequireMainThread();
        var key = Normalize(path);
        var prefix = key.Length == 0 ? "" : key + "/";
        return _entries
            .Where(entry => entry.Path == key || entry.Path.StartsWith(prefix, StringComparison.Ordinal))
            .Select(entry => Fetch(entry, type))
            .OfType<Object>()
            .Distinct()
            .ToArray();
    }

    private static void RequireMainThread()
    {
        if (!DisunityHost.OnMainThread)
            throw new InvalidOperationException("Resources can only be loaded from the main thread");
    }

    private static string Normalize(string path)
    {
        ArgumentNullException.ThrowIfNull(path);
        return path.Replace('\\', '/').Trim('/').ToLowerInvariant();
    }

    /// <summary>Whether the entry's object is a <paramref name="type"/>, from its planned class
    /// or, for classes the engine does not build, from the Unity class name.</summary>
    private bool Matches(Entry entry, Type type)
    {
        if (type == typeof(Object)) return true;
        if (entry.Asset is { } index) return type.IsAssignableFrom(AssetType(_assets[index]));
        var engine = typeof(Object).Assembly.GetType($"UnityEngine.{entry.Class}");
        return engine is not null ? type.IsAssignableFrom(engine) : type.Name == entry.Class;
    }

    private Type AssetType(JsonElement asset) => asset.Str("type") switch
    {
        "TextAsset" => typeof(TextAsset),
        "Texture2D" => typeof(Texture2D),
        "Sprite" => typeof(Sprite),
        "SpriteAtlas" => typeof(UnityEngine.U2D.SpriteAtlas),
        "Script" => ScriptType(asset),
        "Prefab" or "UnplannedPrefab" => typeof(GameObject),
        "Shader" or "UnplannedShader" => typeof(Shader),
        var other => throw new InvalidDataException($"resource plans have no {other} assets"),
    };

    private Assembly ResolveAssembly(string name) => name == "UnityEngine.CoreModule" ? typeof(Object).Assembly : _assemblies(name);

    private Type ScriptType(JsonElement asset)
    {
        var assembly = asset.Str("assembly");
        var name = asset.Str("class");
        var type = ResolveAssembly(assembly).GetType(name.Replace('/', '+'), throwOnError: false);
        if (type is null || !typeof(ScriptableObject).IsAssignableFrom(type) || type.IsAbstract)
            throw new InvalidDataException($"resource {asset.Str("name")}: {assembly} has no ScriptableObject class {name}");
        return type;
    }

    private Object Build(Entry entry, Type type)
    {
        if (entry.Asset is not { } index)
            throw new NotSupportedException($"Resources: {entry.Path} is a {entry.Class}, which the engine does not load yet (asked for {type.Name})");
        var outermost = _binding.Count == 0;
        try
        {
            var asset = Asset(index);
            if (outermost) Activate();
            return asset;
        }
        catch when (outermost)
        {
            foreach (var (created, _) in _binding) _built.Remove(created);
            _binding.Clear();
            throw;
        }
    }

    /// <summary>Registers the script assets this load created, which runs their <c>Awake</c>
    /// and <c>OnEnable</c>, once all of them are bound.</summary>
    private void Activate()
    {
        for (var i = 0; i < _binding.Count; i++) DisunityHost.RegisterAsset(_binding[i].Asset);
        _binding.Clear();
    }

    private Object Asset(int index)
    {
        if (_built.TryGetValue(index, out var built) && (built || _binding.Exists(entry => entry.Index == index))) return built;
        var asset = _assets[index];
        return asset.Str("type") switch
        {
            "TextAsset" => _built[index] = Text(asset),
            "Texture2D" => _built[index] = Texture(asset),
            "Sprite" => _built[index] = PlayerSprite(asset),
            "SpriteAtlas" => _built[index] = Atlas(asset),
            "Prefab" => _built[index] = Prefab(index, asset),
            "UnplannedPrefab" => throw new InvalidOperationException($"Resources: prefab {asset.Str("name")} did not plan: {asset.Str("error")}"),
            "Shader" => _built[index] = PlayerShaders.Build(asset, Difference),
            "UnplannedShader" => throw new InvalidOperationException($"Resources: {asset.Str("error")}"),
            _ => Script(index, asset),
        };
    }

    private static TextAsset Text(JsonElement asset)
    {
        var (name, path) = (asset.Str("name"), asset.Str("file"));
        var (offset, length) = (asset.GetProperty("offset").GetInt64(), asset.GetProperty("length").GetInt64());
        if (length < 0 || length > int.MaxValue)
            throw new InvalidDataException($"resource {name}: {length} bytes cannot be read");
        var bytes = new byte[length];
        ReadBytes(name, path, offset, bytes);
        return new TextAsset(name, bytes);
    }

    /// <summary>Fills <paramref name="destination"/> from <paramref name="path"/> at <paramref name="offset"/>.</summary>
    private static void ReadBytes(string name, string path, long offset, Span<byte> destination)
    {
        using var file = File.OpenRead(path);
        if (offset < 0 || offset > file.Length - destination.Length)
            throw new InvalidDataException($"resource {name}: bytes {offset}+{destination.Length} lie outside {path} ({file.Length} bytes)");
        file.Seek(offset, SeekOrigin.Begin);
        file.ReadExactly(destination);
    }

    private ScriptableObject Script(int index, JsonElement asset)
    {
        var type = ScriptType(asset);
        var instance = (ScriptableObject)Activator.CreateInstance(type, nonPublic: true)!;
        instance.name = asset.Str("name");
        // Kept before its fields bind, so assets that reference each other resolve.
        _built[index] = instance;
        _binding.Add((index, instance));
        using var fields = JsonDocument.Parse(File.ReadAllBytes(Path.Combine(_directory, asset.Str("fields"))));
        new FieldBinder(Resolve, assemblies: ResolveAssembly).Bind(instance, fields.RootElement, $"resource {instance.name}");
        return instance;
    }

    private object Resolve(JsonProperty reference) => reference.Name switch
    {
        "$asset" => Asset(reference.Value.GetInt32()),
        var other => throw new SceneLoadException($"resource plans have no {other} references"),
    };
}
