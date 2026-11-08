using System.Reflection;
using System.Runtime.Loader;

namespace Disunity.Hosting;

/// <summary>
/// Resolves a built player's assemblies in one load context. Unity engine module names
/// resolve to facades built from the player's own module metadata (see
/// <see cref="ModuleFacades"/>). The game and package assemblies the player lists
/// (<see cref="PlayerManifest.GameAssemblies"/>) resolve to its <c>Managed</c> directory,
/// whatever their names. The player's unlisted DLLs, such as <c>System.Core</c> and
/// <c>Mono.Security</c>, are class libraries built for Unity's Mono runtime, and their names are left to
/// .NET. A name that the player neither lists nor ships, and that is not an engine module, resolves to
/// the host's own library directory when that holds an assembly of the name (see the
/// <c>hostLibraries</c> constructor argument); any other name is left to .NET. A context asks only for
/// names it and .NET could not resolve. <see cref="ServeDefaultContext"/> gives the default context the same
/// host libraries, for types the framework's own facades forward to them. Each game assembly it loads
/// also gets the player's native plugin resolution (<see cref="PlayerNativePlugins"/>). With a
/// callback cache, a game assembly that makes delegates for static methods marked <c>MonoPInvokeCallback</c> loads
/// from a rewritten copy in the cache that keeps those delegates alive, as a player's own runtime does.
/// </summary>
public sealed class PlayerAssemblies
{
    private readonly string _managed;
    private readonly Assembly _engine;
    private readonly HashSet<string> _game;
    private readonly IReadOnlyList<string> _listed;
    private readonly PlayerNativePlugins? _native;
    private readonly string? _hostLibraries;
    private readonly CallbackCache? _callbacks;
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
    /// <param name="nativePlugins">
    /// Resolves the native libraries the game and package assemblies import, or null to leave
    /// that to .NET. It registers for each assembly this instance loads.
    /// </param>
    /// <param name="hostLibraries">
    /// The host's directory of class libraries that a Mono player gets from its runtime and does not ship, such as
    /// <c>System.Configuration.ConfigurationManager</c>, or null for none. The host carries them as packages.
    /// </param>
    /// <param name="callbackCache">
    /// The directory that holds game assemblies rewritten so the delegates they pass to native code stay callable (see
    /// <see cref="PlayerCallbackRewriter"/>), or null to load the player's assemblies as they are.
    /// </param>
    /// <param name="log">Receives one line for each assembly that is rewritten, or null for none.</param>
    public PlayerAssemblies(string managedDirectory, Assembly engine, IEnumerable<string> gameAssemblies,
        PlayerNativePlugins? nativePlugins = null, string? hostLibraries = null, string? callbackCache = null,
        Action<string>? log = null)
    {
        if (!Directory.Exists(managedDirectory))
            throw new DirectoryNotFoundException($"no player Managed directory at {managedDirectory}");
        _managed = managedDirectory;
        _engine = engine;
        _listed = gameAssemblies.ToList();
        _game = new HashSet<string>(_listed, StringComparer.Ordinal);
        _native = nativePlugins;
        _hostLibraries = hostLibraries;
        _callbacks = callbackCache is null ? null : new CallbackCache(callbackCache, log);
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
    /// the player ships, the player's assembly for a listed game or package name, the host's
    /// library for a name neither listed nor shipped by the player, or null for any other name.
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
        if (!_game.Contains(simple)) return ResolveHostLibrary(context, name);
        if (!File.Exists(path))
            throw new FileNotFoundException($"the player lists {simple} but its Managed directory has no {simple}.dll", path);
        var assembly = context.LoadFromAssemblyPath(_callbacks?.Resolve(simple, path) ?? path);
        _native?.Register(assembly);
        return assembly;
    }

    /// <summary>
    /// Loads every listed game and package assembly into the attached context, in the order the player lists them,
    /// as a player's runtime does at startup, so code that enumerates the loaded assemblies to discover types sees
    /// them all before it first runs (the engine's choice is to load them before any startup hook). A listed name
    /// that cannot be loaded (missing from Managed, not a .NET image, or failing to load) is passed to
    /// <paramref name="report"/> with the reason and skipped. Returns how many loaded.
    /// </summary>
    public int LoadListed(Action<string> report)
    {
        var context = _context ?? throw new InvalidOperationException("PlayerAssemblies.LoadListed needs an attached load context");
        var loaded = 0;
        foreach (var name in _listed)
        {
            try
            {
                context.LoadFromAssemblyName(new AssemblyName(name));
                loaded++;
            }
            catch (Exception error) when (error is FileNotFoundException or BadImageFormatException or FileLoadException)
            {
                report($"listed assembly {name} did not load: {error.GetType().Name}: {error.Message}");
            }
        }
        return loaded;
    }

    /// <summary>
    /// Registers <see cref="ResolveHostLibrary"/> with the default load context. The framework's own facades,
    /// such as <c>System.Configuration</c>, forward types to the host libraries from inside the default context,
    /// which the player's context does not serve, so a type a player's package reaches through such a facade needs
    /// the default context to find the library as well.
    /// </summary>
    public void ServeDefaultContext() => AssemblyLoadContext.Default.Resolving += ResolveHostLibrary;

    /// <summary>
    /// Loads the host library of a name into <paramref name="context"/>, or returns null when the player lists
    /// the name, ships it, or when it is an engine module or the engine itself, or when the host has no such library.
    /// </summary>
    public Assembly? ResolveHostLibrary(AssemblyLoadContext context, AssemblyName name)
    {
        if (_hostLibraries is null || name.Name is not { Length: > 0 } simple) return null;
        if (simple == _engine.GetName().Name || ModuleFacades.IsEngineModule(simple) || _game.Contains(simple)) return null;
        if (File.Exists(Path.Combine(_managed, simple + ".dll"))) return null;
        var path = Path.Combine(_hostLibraries, simple + ".dll");
        return File.Exists(path) ? context.LoadFromAssemblyPath(path) : null;
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
