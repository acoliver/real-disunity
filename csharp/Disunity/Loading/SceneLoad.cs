using System.Reflection;
using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>How <see cref="DisunityHost.LoadScene"/> finds what a plan names but does not hold:
/// the compiled game scripts, and the bytes of package fonts by name (Unity's Arial).
/// Texture <c>i</c> of the plan gets identity <see cref="FirstTextureIdentity"/> + i.</summary>
public sealed record SceneLoadOptions(Assembly Scripts, Func<string, byte[]> Font)
{
    public ulong FirstTextureIdentity { get; init; } = 1;

    /// <summary>The assembly with a given name, for scripts a player's plan names by assembly
    /// and full class name. Project plans name scripts by file and use <see cref="Scripts"/>.</summary>
    public Func<string, Assembly>? Assemblies { get; init; }
}

/// <summary>A plan texture with the identity its sprites use, for the host to upload.</summary>
public sealed record SceneTexture(ulong Identity, string Path, uint Width, uint Height);

/// <summary>
/// A scene built from a plan. Its objects are live in the world; its prefab templates stay
/// unpublished for <c>Instantiate</c> until <see cref="Dispose"/> destroys them.
/// </summary>
public sealed class LoadedScene : IDisposable
{
    private readonly Construction templates;
    internal LoadedScene(Construction templates, IReadOnlyList<GameObject> objects, IReadOnlyList<SceneTexture> textures,
        IReadOnlyList<string> unregisteredAxes, IReadOnlyList<string> differences)
    {
        this.templates = templates;
        Objects = objects;
        Textures = textures;
        UnregisteredAxes = unregisteredAxes;
        Differences = differences;
    }
    /// <summary>Every object of the plan, scene and template, in plan order.</summary>
    public IReadOnlyList<GameObject> Objects { get; }
    public IReadOnlyList<SceneTexture> Textures { get; }
    /// <summary>Input axes not registered because the engine cannot deliver one of their inputs;
    /// reading one fails as an unregistered axis does.</summary>
    public IReadOnlyList<string> UnregisteredAxes { get; }
    /// <summary>Authored presentation settings the engine does not reproduce, one line each.</summary>
    public IReadOnlyList<string> Differences { get; }
    public void Dispose() => templates.Dispose();
}

/// <summary>A plan the loader cannot build, naming the object, component or field.</summary>
public sealed class SceneLoadException(string message, Exception? inner = null) : Exception(message, inner);

public static partial class DisunityHost
{
    /// <summary>
    /// Builds the scene a <c>unity-import plan</c> describes: objects, engine components,
    /// scripts with their serialized fields, assets, sprites and input axes. Scene objects wake
    /// together when everything is bound, as a Unity scene load does. On failure nothing stays
    /// in the world.
    /// </summary>
    public static LoadedScene LoadScene(string plan, SceneLoadOptions options)
    {
        ArgumentNullException.ThrowIfNull(plan);
        ArgumentNullException.ThrowIfNull(options);
        EnsureWorld();
        using var document = JsonDocument.Parse(plan);
        return new SceneLoader(document.RootElement, options).Load();
    }
}

/// <summary>Reads the plan's JSON values.</summary>
internal static class PlanJson
{
    public static string Str(this JsonElement value, string name) => value.GetProperty(name).GetString() ?? "";
    public static float Float(this JsonElement value, string name) => value.GetProperty(name).GetSingle();
    public static int Int(this JsonElement value, string name) => value.GetProperty(name).GetInt32();
    public static bool Bool(this JsonElement value, string name) => value.GetProperty(name).GetBoolean();
    public static int? Index(this JsonElement value, string name) =>
        value.GetProperty(name).ValueKind == JsonValueKind.Null ? null : value.GetProperty(name).GetInt32();
    public static float[] Floats(this JsonElement value, string name) =>
        value.GetProperty(name).EnumerateArray().Select(item => item.GetSingle()).ToArray();
    public static Vector2 Vec2(this JsonElement value, string name)
    {
        var v = value.Floats(name);
        return new Vector2(v[0], v[1]);
    }
    public static Color Rgba(this JsonElement value, string name)
    {
        var v = value.Floats(name);
        return new Color(v[0], v[1], v[2], v[3]);
    }
    public static T Variant<T>(this JsonElement value, string name) where T : struct, Enum =>
        Enum.Parse<T>(value.Str(name));
}
