using System.Reflection;
using System.Runtime.Loader;

namespace Disunity.Hosting;

/// <summary>
/// Resolves a built player's assemblies in one load context. Unity engine module names
/// resolve to facades built from the player's own module metadata (see
/// <see cref="ModuleFacades"/>); other names resolve to the assemblies in the player's
/// <c>Managed</c> directory. A context asks only for names it and .NET could not resolve.
/// Framework names (<c>mscorlib</c>, <c>netstandard</c>, <c>System</c> and <c>System.*</c>)
/// are never taken from the player, whose copies are built for Unity's Mono runtime: a
/// framework assembly .NET lacks stays unresolved and fails to load by name.
/// </summary>
public sealed class PlayerAssemblies
{
    private readonly string _managed;
    private readonly Assembly _engine;
    private readonly Dictionary<string, Assembly> _facades = new(StringComparer.Ordinal);
    // Resolution can run on any thread that first touches a type; the lock makes each
    // facade build once. Game assemblies need no lock: the context loads each name once.
    private readonly object _gate = new();
    private AssemblyLoadContext? _context;

    /// <param name="managedDirectory">The player's <c>&lt;Game&gt;_Data/Managed</c> directory.</param>
    /// <param name="engine">
    /// The Disunity.Engine assembly to forward to. Facades reference it by name, so the
    /// context must resolve that name to this assembly, as the default context does.
    /// </param>
    public PlayerAssemblies(string managedDirectory, Assembly engine)
    {
        if (!Directory.Exists(managedDirectory))
            throw new DirectoryNotFoundException($"no player Managed directory at {managedDirectory}");
        _managed = managedDirectory;
        _engine = engine;
    }

    /// <summary>
    /// Resolves the player's assembly names in <paramref name="context"/> from now on.
    /// An instance serves one context, since its facades belong to that context.
    /// </summary>
    public void Attach(AssemblyLoadContext context)
    {
        if (_context is not null)
            throw new InvalidOperationException("PlayerAssemblies resolves names for one load context; create another for a second context");
        _context = context;
        context.Resolving += Resolve;
    }

    /// <summary>
    /// The assembly for a name the context could not resolve: a facade for an engine module,
    /// the player's assembly for another name it ships, or null for a framework name or a
    /// name the player does not ship.
    /// </summary>
    public Assembly? Resolve(AssemblyLoadContext context, AssemblyName name)
    {
        if (!ReferenceEquals(context, _context))
            throw new InvalidOperationException("PlayerAssemblies resolves names for one load context, the one attached");
        if (name.Name is not { Length: > 0 } simple || IsFramework(simple)) return null;
        var path = Path.Combine(_managed, simple + ".dll");
        if (!File.Exists(path)) return null;
        return ModuleFacades.IsEngineModule(simple)
            ? Facade(context, simple, path)
            : context.LoadFromAssemblyPath(path);
    }

    private static bool IsFramework(string name) =>
        name is "mscorlib" or "netstandard" or "System"
        || name.StartsWith("System.", StringComparison.Ordinal);

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
