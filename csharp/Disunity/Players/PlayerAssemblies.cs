using System.Reflection;
using System.Runtime.Loader;

namespace Disunity.Hosting;

/// <summary>
/// Resolves a built player's assemblies in a load context. Unity engine module names resolve
/// to facades built from the player's own module metadata (see <see cref="ModuleFacades"/>);
/// other names resolve to the assemblies in the player's <c>Managed</c> directory. Only names
/// the context and .NET cannot resolve reach it, so framework assemblies always come from
/// .NET, never from the class libraries a player ships for Unity's Mono runtime.
/// </summary>
public sealed class PlayerAssemblies
{
    private readonly string _managed;
    private readonly Assembly _engine;
    private readonly Dictionary<string, Assembly> _facades = new(StringComparer.Ordinal);
    private readonly object _gate = new();

    /// <param name="managedDirectory">The player's <c>&lt;Game&gt;_Data/Managed</c> directory.</param>
    /// <param name="engine">The Disunity.Engine assembly the facades forward to.</param>
    public PlayerAssemblies(string managedDirectory, Assembly engine)
    {
        if (!Directory.Exists(managedDirectory))
            throw new DirectoryNotFoundException($"no player Managed directory at {managedDirectory}");
        _managed = managedDirectory;
        _engine = engine;
    }

    /// <summary>Resolves the player's assembly names in <paramref name="context"/> from now on.</summary>
    public void Attach(AssemblyLoadContext context) => context.Resolving += Resolve;

    /// <summary>
    /// The assembly for a name the context could not resolve: a facade for an engine module,
    /// the player's assembly for another name it ships, or null for a name it does not ship.
    /// </summary>
    public Assembly? Resolve(AssemblyLoadContext context, AssemblyName name)
    {
        if (name.Name is not { Length: > 0 } simple) return null;
        var path = Path.Combine(_managed, simple + ".dll");
        if (!File.Exists(path)) return null;
        return ModuleFacades.IsEngineModule(simple)
            ? Facade(context, simple, path)
            : context.LoadFromAssemblyPath(path);
    }

    private Assembly Facade(AssemblyLoadContext context, string name, string path)
    {
        lock (_gate)
        {
            if (_facades.TryGetValue(name, out var loaded)) return loaded;
            UnityModule module;
            using (var stream = File.OpenRead(path)) module = ModuleFacades.Read(stream);
            using var image = new MemoryStream(ModuleFacades.Build(module, _engine));
            var facade = context.LoadFromStream(image);
            _facades[name] = facade;
            return facade;
        }
    }
}
