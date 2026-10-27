using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

/// <summary>
/// A static method a player runs once during startup, from
/// <c>RuntimeInitializeOnLoads.json</c>: its assembly, the type's full name, the method
/// name and the startup phase it belongs to.
/// </summary>
public sealed record RuntimeHook(string Assembly, string Type, string Method, RuntimeInitializeLoadType Phase)
{
    public override string ToString() => $"[{Assembly}]{Type}.{Method}";
}

/// <summary>
/// What a built player's data directory lists about its managed code: the game and package
/// assemblies it loads at startup, in order, from <c>ScriptingAssemblies.json</c>, and its
/// startup hooks.
/// </summary>
public sealed record PlayerManifest(IReadOnlyList<string> GameAssemblies, IReadOnlyList<RuntimeHook> Hooks)
{
    /// <summary>The <c>ScriptingAssemblies.json</c> type of a Unity engine module.</summary>
    private const int EngineModuleType = 2;

    /// <summary>The <c>ScriptingAssemblies.json</c> type of a game or package assembly.</summary>
    private const int GameAssemblyType = 16;

    /// <summary>Reads the manifest files from a player's <c>&lt;Game&gt;_Data</c> directory.</summary>
    public static PlayerManifest Read(string dataDirectory)
    {
        using var assemblies = File.OpenRead(Path.Combine(dataDirectory, "ScriptingAssemblies.json"));
        using var hooks = File.OpenRead(Path.Combine(dataDirectory, "RuntimeInitializeOnLoads.json"));
        return new PlayerManifest(ReadAssemblies(assemblies), ReadHooks(hooks));
    }

    /// <summary>
    /// The game and package assembly names in <c>ScriptingAssemblies.json</c>, in order. Every
    /// engine module entry must be a name <see cref="ModuleFacades.IsEngineModule"/> accepts and
    /// every game entry one it rejects, so facades and game code never disagree on a name.
    /// </summary>
    public static IReadOnlyList<string> ReadAssemblies(Stream json) =>
        Parse(json, "ScriptingAssemblies.json", Assemblies);

    /// <summary>The startup hooks in <c>RuntimeInitializeOnLoads.json</c>, in listed order.</summary>
    public static IReadOnlyList<RuntimeHook> ReadHooks(Stream json) =>
        Parse(json, "RuntimeInitializeOnLoads.json", root => root.GetProperty("root").EnumerateArray().Select(ReadHook).ToList());

    /// <summary>
    /// Parses a manifest file, reporting malformed JSON and missing or mistyped values as an
    /// <see cref="InvalidDataException"/> that names the file.
    /// </summary>
    private static T Parse<T>(Stream json, string file, Func<JsonElement, T> read)
    {
        try
        {
            using var document = JsonDocument.Parse(json);
            return read(document.RootElement);
        }
        catch (Exception error) when (error is JsonException or KeyNotFoundException or InvalidOperationException or FormatException)
        {
            throw new InvalidDataException($"{file} is malformed: {error.Message}", error);
        }
    }

    private static IReadOnlyList<string> Assemblies(JsonElement root)
    {
        var names = root.GetProperty("names").EnumerateArray().Select(ReadText).ToList();
        var types = root.GetProperty("types").EnumerateArray().Select(type => type.GetInt32()).ToList();
        if (names.Count != types.Count)
            throw new InvalidDataException($"ScriptingAssemblies.json lists {names.Count} names and {types.Count} types");
        var game = new List<string>();
        for (var i = 0; i < names.Count; i++)
        {
            var name = AssemblyName(names[i]);
            var engine = ModuleFacades.IsEngineModule(name);
            if (types[i] == GameAssemblyType && !engine) game.Add(name);
            else if (types[i] != EngineModuleType || !engine)
                throw new InvalidDataException($"ScriptingAssemblies.json lists {names[i]} with type {types[i]}, which does not match its name");
        }
        return game;
    }

    private static RuntimeHook ReadHook(JsonElement entry)
    {
        var phase = entry.GetProperty("loadTypes").GetInt32();
        if (!Enum.IsDefined(typeof(RuntimeInitializeLoadType), phase))
            throw new InvalidDataException($"RuntimeInitializeOnLoads.json has unknown load type {phase}");
        var space = ReadText(entry.GetProperty("nameSpace"));
        var type = ReadText(entry.GetProperty("className"));
        return new RuntimeHook(
            ReadText(entry.GetProperty("assemblyName")),
            space.Length == 0 ? type : $"{space}.{type}",
            ReadText(entry.GetProperty("methodName")),
            (RuntimeInitializeLoadType)phase);
    }

    private static string ReadText(JsonElement value) =>
        value.GetString() ?? throw new InvalidDataException("a player manifest has a null where a name belongs");

    private static string AssemblyName(string file) =>
        file.EndsWith(".dll", StringComparison.Ordinal)
            ? file[..^4]
            : throw new InvalidDataException($"ScriptingAssemblies.json lists {file}, which is not a .dll");
}
