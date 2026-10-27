using System.Reflection;
using System.Runtime.Loader;

namespace Disunity.Hosting;

/// <summary>
/// Resolves a built player's assemblies in one load context. Unity engine module names
/// resolve to facades built from the player's own module metadata (see
/// <see cref="ModuleFacades"/>). The game and package assemblies the player lists
/// (<see cref="PlayerManifest.GameAssemblies"/>) resolve to its <c>Managed</c> directory,
/// whatever their names. Any other name is left to .NET: the player's unlisted DLLs, such as
/// <c>System.Core</c> and <c>Mono.Security</c>, are class libraries built for Unity's Mono
/// runtime. A context asks only for names it and .NET could not resolve.
/// </summary>
public sealed class PlayerAssemblies
{
    private readonly string _managed;
    private readonly Assembly _engine;
    private readonly HashSet<string> _game;
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
    /// <param name="gameAssemblies">The names of the game and package assemblies the player lists.</param>
    public PlayerAssemblies(string managedDirectory, Assembly engine, IEnumerable<string> gameAssemblies)
    {
        if (!Directory.Exists(managedDirectory))
            throw new DirectoryNotFoundException($"no player Managed directory at {managedDirectory}");
        _managed = managedDirectory;
        _engine = engine;
        _game = new HashSet<string>(gameAssemblies, StringComparer.Ordinal);
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
    /// The assembly for a name the context could not resolve: a facade for an engine module
    /// the player ships, the player's assembly for a listed game or package name, or null for
    /// any other name.
    /// </summary>
    /// <exception cref="FileNotFoundException">The player lists the name but does not ship it.</exception>
    public Assembly? Resolve(AssemblyLoadContext context, AssemblyName name)
    {
        if (!ReferenceEquals(context, _context))
            throw new InvalidOperationException("PlayerAssemblies resolves names for one load context, the one attached");
        if (name.Name is not { Length: > 0 } simple) return null;
        var path = Path.Combine(_managed, simple + ".dll");
        if (ModuleFacades.IsEngineModule(simple))
            return File.Exists(path) ? Facade(context, simple, path) : null;
        if (!_game.Contains(simple)) return null;
        if (!File.Exists(path))
            throw new FileNotFoundException($"the player lists {simple} but its Managed directory has no {simple}.dll", path);
        return context.LoadFromAssemblyPath(path);
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
