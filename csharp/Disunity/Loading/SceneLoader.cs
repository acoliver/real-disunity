using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// Builds one plan. Scene objects go into a construction that activates once every component
/// and field is bound; template objects into one that is never activated.
/// </summary>
internal sealed partial class SceneLoader(JsonElement plan, SceneLoadOptions options)
{
    private readonly List<GameObject> objects = new();
    private readonly HashSet<GameObject> templateObjects = new();
    /// <summary>Each object's planned components in order; null where the plan's component has
    /// no engine object, such as a CanvasScaler applied to its canvas.</summary>
    private readonly List<object?[]> components = new();
    private readonly List<object> assets = new();
    private readonly List<Sprite> sprites = new();
    private readonly Dictionary<string, Font> fonts = new();
    private readonly List<string> differences = new();
    private readonly List<(MonoBehaviour Script, JsonElement Fields, string Where)> scripts = new();
    private readonly Dictionary<string, Type> types = new();

    public LoadedScene Load()
    {
        var scene = DisunityHost.BeginConstruction();
        var templates = DisunityHost.BeginConstruction();
        try
        {
            var textures = Textures();
            CreateSprites(textures);
            CreateAssets();
            CreateObjects(scene, templates);
            CreateComponents();
            BindFields();
            var unregistered = InputAxes.Register(plan.GetProperty("axes"));
            ApplyLayerNames();
            scene.Activate();
            return new LoadedScene(templates, objects, textures, unregistered, differences, templateObjects);
        }
        catch (Exception failure)
        {
            templates.Dispose();
            scene.Dispose();
            if (failure is SceneLoadException) throw;
            var cause = Cause(failure);
            throw new SceneLoadException($"{plan.Str("scene")}: {Describe(cause)}", cause);
        }
    }

    /// <summary>Builds a plan of template objects only: a player's prefab. Nothing is activated, so the
    /// objects stay unpublished for <c>Instantiate</c> and belong to no scene.</summary>
    public LoadedScene LoadTemplates()
    {
        var templates = DisunityHost.BeginConstruction();
        try
        {
            CreateObjects(templates, templates);
            CreateComponents();
            BindFields();
            return new LoadedScene(templates, objects, [], [], differences, templateObjects);
        }
        catch (Exception failure)
        {
            templates.Dispose();
            if (failure is SceneLoadException) throw;
            var cause = Cause(failure);
            throw new SceneLoadException($"{plan.Str("scene")}: {Describe(cause)}", cause);
        }
    }

    /// <summary>A plan that carries layer names replaces every layer name with them, so scripts that look a
    /// layer up in Awake find it. A plan without them (a project plan) leaves what the host set.</summary>
    private void ApplyLayerNames()
    {
        if (!plan.TryGetProperty("layers", out var layers)) return;
        DisunityHost.SetLayerNames(layers.EnumerateArray().Select(name => name.GetString() ?? "").ToList());
    }

    private List<SceneTexture> Textures() => plan.GetProperty("textures").EnumerateArray()
        .Select((texture, i) => new SceneTexture(options.FirstTextureIdentity + (ulong)i, texture.Str("path"),
            texture.GetProperty("width").GetUInt32(), texture.GetProperty("height").GetUInt32()))
        .ToList();

    private void CreateSprites(List<SceneTexture> textures)
    {
        foreach (var sprite in plan.GetProperty("sprites").EnumerateArray())
        {
            var texture = textures[sprite.Int("texture")];
            var rect = sprite.Floats("rect");
            var created = DisunityHost.CreateSprite(texture.Identity, texture.Width, texture.Height,
                new Rect(rect[0], rect[1], rect[2], rect[3]), sprite.Vec2("pivot"), sprite.Float("pixels_per_unit"));
            if (sprite.TryGetProperty("name", out var name) && name.GetString() is { } text) DisunityHost.NameSprite(created, text);
            sprites.Add(created);
        }
    }

    private void CreateAssets()
    {
        foreach (var asset in plan.GetProperty("assets").EnumerateArray())
            assets.Add(asset.Str("type") switch
            {
                "PhysicsMaterial2D" => Material(asset),
                "Script" => Asset(asset),
                var other => throw new SceneLoadException($"asset type {other} is not supported"),
            });
    }

    private static PhysicsMaterial2D Material(JsonElement asset)
    {
        var name = asset.Str("name");
        if (asset.Str("friction_combine") != "Mean" || asset.Str("bounce_combine") != "Maximum")
            throw new SceneLoadException($"physics material {name}: only Unity's default Mean friction and Maximum bounce combining are supported");
        return new PhysicsMaterial2D(name) { friction = asset.Float("friction"), bounciness = asset.Float("bounciness") };
    }

    private ScriptableObject Asset(JsonElement asset)
    {
        var where = $"asset {asset.Str("name")} ({asset.Str("script")})";
        var type = ScriptType(asset, typeof(ScriptableObject), where);
        var instance = ScriptableObject.CreateInstance(type);
        instance.name = asset.Str("name");
        return instance;
    }

    /// <summary>The class a script file names in the game assembly: Unity requires the class
    /// to match the file name, in any namespace. A player's script names its assembly and the
    /// class's full name instead.</summary>
    private Type ScriptType(JsonElement script, Type expected, string where)
    {
        var name = script.Str("class");
        if (script.TryGetProperty("assembly", out var assembly)) return PlayerScriptType(assembly.GetString()!, name, expected, where);
        if (types.TryGetValue(name, out var known)) return known;
        var matches = options.Scripts.GetTypes().Where(t => t.Name == name && expected.IsAssignableFrom(t) && !t.IsAbstract).ToList();
        if (matches.Count != 1)
            throw new SceneLoadException($"{where}: the game assembly has {matches.Count} {expected.Name} classes named {name}");
        return types[name] = matches[0];
    }

    /// <summary>A class by assembly and full name, nested classes joined by <c>/</c>.</summary>
    private Type PlayerScriptType(string assembly, string fullName, Type expected, string where)
    {
        var key = $"{assembly}:{fullName}";
        if (types.TryGetValue(key, out var known)) return known;
        var resolve = options.Assemblies
            ?? throw new SceneLoadException($"{where}: the plan names assembly {assembly}, and the loader has no assembly resolver");
        var type = resolve(assembly).GetType(fullName.Replace('/', '+'), throwOnError: false);
        if (type is null || type.IsAbstract || !expected.IsAssignableFrom(type))
            throw new SceneLoadException($"{where}: {assembly} has no {expected.Name} class {fullName}");
        return types[key] = type;
    }

    private void CreateObjects(Construction scene, Construction templates)
    {
        foreach (var value in plan.GetProperty("objects").EnumerateArray())
        {
            var created = (value.Bool("template") ? templates : scene).CreateGameObject();
            if (value.Bool("template")) templateObjects.Add(created);
            created.name = value.Str("name");
            created.tag = value.Str("tag");
            created.layer = value.Int("layer");
            if (value.GetProperty("rect").ValueKind != JsonValueKind.Null) created.AddComponent<RectTransform>();
            if (value.Index("parent") is { } parent) created.transform.SetParent(objects[parent].transform, false);
            if (value.GetProperty("rect").ValueKind != JsonValueKind.Null) Layout((RectTransform)created.transform, value.GetProperty("rect"));
            var p = value.Floats("position");
            var r = value.Floats("rotation");
            var s = value.Floats("scale");
            created.transform.localPosition = new Vector3(p[0], p[1], p[2]);
            created.transform.localRotation = new Quaternion(r[0], r[1], r[2], r[3]);
            created.transform.localScale = new Vector3(s[0], s[1], s[2]);
            if (!value.Bool("active")) created.SetActive(false);
            objects.Add(created);
        }
    }

    private void CreateComponents()
    {
        var values = plan.GetProperty("objects").EnumerateArray().ToList();
        foreach (var value in values) components.Add(new object?[value.GetProperty("components").GetArrayLength()]);
        for (var i = 0; i < values.Count; i++)
        {
            var list = values[i].GetProperty("components").EnumerateArray().ToList();
            for (var j = 0; j < list.Count; j++)
            {
                var where = $"{objects[i].name} component {j} ({list[j].Str("type")})";
                try { components[i][j] = Component(objects[i], list[j], where); }
                catch (Exception error) when (error is not SceneLoadException)
                {
                    var cause = Cause(error);
                    throw new SceneLoadException($"{where}: {Describe(cause)}", cause);
                }
            }
        }
    }

    /// <summary>Builds a scene script. A script whose class cannot load, because it uses an engine type the engine
    /// does not define, is left off its object like a missing script: the rest of the scene loads, the plan's
    /// differences and the log name it, and references to it bind null.</summary>
    private object Script(GameObject owner, JsonElement value, string where)
    {
        MonoBehaviour script;
        Type type;
        try
        {
            type = ScriptType(value, typeof(MonoBehaviour), where);
            script = (MonoBehaviour)(Activator.CreateInstance(type, nonPublic: true)
                ?? throw new SceneLoadException($"{where}: cannot construct {type}"));
        }
        catch (Exception error) when (Unloadable(error) is { } cause)
        {
            var name = value.Str("class");
            differences.Add($"{where}: script {name} is not built: {cause.Message}");
            Debug.LogWarning($"The script {name} on {owner.name} cannot load and is left off: {cause.Message}");
            return new UnbuiltComponent(name, Script: true);
        }
        DisunityHost.AttachComponent<Component>(owner, script);
        if (!value.Bool("enabled")) script.enabled = false;
        scripts.Add((script, value.GetProperty("fields"), $"{owner.name}.{type.Name}"));
        return script;
    }

    /// <summary>The exception a reflection call wrapped, or the exception itself.</summary>
    private static Exception Cause(Exception error) =>
        error is System.Reflection.TargetInvocationException { InnerException: { } inner } ? inner : error;

    /// <summary>The exception's type and message followed by those of each inner exception, so a failed type
    /// initializer names what it tripped on.</summary>
    private static string Describe(Exception error)
    {
        var parts = new List<string>();
        for (Exception? e = error; e is not null && parts.Count < 6; e = e.InnerException) parts.Add($"{e.GetType().Name}: {e.Message}");
        return string.Join(" <- ", parts);
    }

    private static TypeLoadException? Unloadable(Exception error) => error switch
    {
        TypeLoadException load => load,
        System.Reflection.TargetInvocationException { InnerException: TypeLoadException load } => load,
        _ => null,
    };

    private void BindFields()
    {
        var binder = new FieldBinder(Resolve, differences.Add);
        foreach (var (script, fields, where) in scripts) binder.Bind(script, fields, where);
        var planned = plan.GetProperty("assets").EnumerateArray().ToList();
        for (var i = 0; i < planned.Count; i++)
            if (assets[i] is ScriptableObject asset)
                binder.Bind(asset, planned[i].GetProperty("fields"), $"asset {asset.name}");
    }

    /// <summary>The object a plan reference names: <c>{"$object": i}</c> and its kin.</summary>
    private object Resolve(JsonProperty reference) => reference.Name switch
    {
        "$object" => objects[reference.Value.GetInt32()],
        "$transform" => objects[reference.Value.GetInt32()].transform,
        "$component" => ComponentReference(reference.Value[0].GetInt32(), reference.Value[1].GetInt32()),
        "$asset" => options.ResourceAsset is { } resource ? resource(reference.Value.GetInt32()) : assets[reference.Value.GetInt32()],
        "$sprite" => sprites[reference.Value.GetInt32()],
        "$font" => Font(reference.Value.GetString() ?? ""),
        var other => throw new SceneLoadException($"references {other}, which scripts cannot hold"),
    };

    /// <summary>A built component, or for one the player's plan marks Unsupported, a stand-in
    /// the binder turns into a null reference with a difference. Other unbuilt components, such as
    /// UI event components, cannot be referenced.</summary>
    private object ComponentReference(int owner, int index)
    {
        if (components[owner][index] is { } built) return built;
        var planned = plan.GetProperty("objects")[owner].GetProperty("components")[index];
        if (planned.Str("type") == "Unsupported") return new UnbuiltComponent(planned.Str("class"));
        throw new SceneLoadException($"component [{owner}, {index}] has no engine object scripts can reference");
    }

    private Font Font(string name)
    {
        if (!fonts.TryGetValue(name, out var font))
            fonts[name] = font = DisunityHost.RegisterFont(options.Font(name));
        return font;
    }
}
