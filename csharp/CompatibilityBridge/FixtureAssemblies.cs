using System.Reflection;
using System.Runtime.Loader;
using System.Text.Json;
using CompatibilitySupport;
using UnityEngine;

namespace CompatibilityBridge;

internal sealed class FixtureAssemblies : AssemblyLoadContext, IDisposable
{
    private readonly List<Assembly> _assemblies = new();
    private readonly List<string> _directories = new();
    private readonly List<FieldInfo> _staticRoots = new();
    public int BindingCount => _staticRoots.Count;
    internal FixtureAssemblies() : base(isCollectible: true) { }
    public FixtureAssemblies(JsonElement paths) : this() => LoadPaths(paths);
    internal void LoadPaths(JsonElement paths)
    {
        foreach (var item in paths.EnumerateArray())
        {
            var path = Path.GetFullPath(item.GetString()!);
            if (!File.Exists(path)) throw new FileNotFoundException("fixture assembly not found", path);
            _directories.Add(Path.GetDirectoryName(path)!);
            _assemblies.Add(LoadFromAssemblyPath(path));
        }
        if (_assemblies.Count == 0) throw new InvalidDataException("fixture assemblies must not be empty");
        _staticRoots.AddRange(_assemblies.SelectMany(a => a.GetTypes()).SelectMany(t => t.GetFields(BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic | BindingFlags.DeclaredOnly))
            .Where(f => typeof(UnityEngine.Object).IsAssignableFrom(f.FieldType)));
    }
    protected override Assembly? Load(AssemblyName name)
    {
        if (name.Name == typeof(MonoBehaviour).Assembly.GetName().Name) return typeof(MonoBehaviour).Assembly;
        if (name.Name == typeof(ScriptSession).Assembly.GetName().Name) return typeof(ScriptSession).Assembly;
        foreach (var dir in _directories)
        {
            var path = Path.Combine(dir, name.Name + ".dll");
            if (File.Exists(path)) return LoadFromAssemblyPath(path);
        }
        return null;
    }
    public Type ResolveType(string name)
    {
        var matches = _assemblies.Select(a => a.GetType(name)).Where(t => t is not null).ToArray();
        if (matches.Length > 1) throw new InvalidDataException($"ambiguous fixture type '{name}'");
        return matches.SingleOrDefault() ?? typeof(MonoBehaviour).Assembly.GetType(name)
            ?? throw new TypeLoadException($"fixture type '{name}' is absent from supplied assemblies");
    }
    public void Dispose()
    {
        var failures = new CleanupFailures();
        foreach (var field in _staticRoots.ToArray())
        {
            if (!field.IsInitOnly && !field.IsLiteral) failures.Run(() => field.SetValue(null, null));
            try { if (field.GetValue(null) is null) _staticRoots.Remove(field); }
            catch (Exception error) { failures.Add(error); }
        }
        if (_staticRoots.Count == 0)
        {
            _assemblies.Clear();
            _directories.Clear();
            Unload();
        }
        failures.ThrowIfAny();
    }
}
