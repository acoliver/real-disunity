using System.Reflection;
using System.Runtime.InteropServices;

namespace Disunity.Hosting;

/// <summary>The operating systems whose native library file names differ.</summary>
internal enum NativePlatform { Windows, Linux, MacOs }

/// <summary>The operating system's native library loader, the boundary tests replace.</summary>
internal interface INativeLoader
{
    /// <summary>The runtime's default probing for <paramref name="name"/> from <paramref name="assembly"/>, or zero.</summary>
    nint Probe(string name, Assembly assembly, DllImportSearchPath? searchPath);

    /// <summary>Loads the library file at <paramref name="path"/>; throws when the file does not load.</summary>
    nint Load(string path);
}

internal sealed class SystemNativeLoader : INativeLoader
{
    public nint Probe(string name, Assembly assembly, DllImportSearchPath? searchPath) =>
        System.Runtime.InteropServices.NativeLibrary.TryLoad(name, assembly, searchPath, out var handle) ? handle : 0;

    public nint Load(string path) => System.Runtime.InteropServices.NativeLibrary.Load(path);
}

/// <summary>
/// Finds the native libraries a player's game and package assemblies import. For a library
/// name the runtime's default probing does not find, it searches the player's
/// <c>&lt;Game&gt;_Data/Plugins/&lt;arch&gt;</c> and <c>&lt;Game&gt;_Data/Plugins</c> directories for the
/// platform's file names, so a Windows host loads the game's own <c>AkSoundEngine.dll</c> and a
/// macOS build its own plugin. Failing that, it uses the engine's substitute for the name, if it
/// has one (<see cref="Substitutes"/>), from the substitute directory the host passes in: the
/// directory of the engine library. Any other name stays unresolved, so the runtime reports
/// <see cref="DllNotFoundException"/> as it does without this class.
/// </summary>
public sealed class PlayerNativePlugins
{
    /// <summary>The engine's substitute library for each plugin name the player may lack, the only
    /// place the pairing lives. <c>AkSoundEngine</c> is Wwise's native sound engine; the substitute
    /// plays nothing and reports zero for every call.</summary>
    internal static readonly IReadOnlyDictionary<string, string> Substitutes = new Dictionary<string, string>(StringComparer.Ordinal)
    {
        ["AkSoundEngine"] = "worldrunner_silent_wwise",
    };

    private readonly string _data;
    private readonly string? _substitutes;
    private readonly Action<string> _log;
    private readonly INativeLoader _loader;
    private readonly NativePlatform _platform;
    private readonly string _architecture;
    private readonly object _gate = new();
    private readonly Dictionary<string, nint> _found = new(StringComparer.Ordinal);
    [ThreadStatic] private static bool _probing;

    /// <param name="dataDirectory">The player's <c>&lt;Game&gt;_Data</c> directory.</param>
    /// <param name="substituteDirectory">Where the engine's substitute libraries are, or null for none.</param>
    /// <param name="log">Receives one line each time a substitute is used.</param>
    public PlayerNativePlugins(string dataDirectory, string? substituteDirectory, Action<string> log)
        : this(dataDirectory, substituteDirectory, log, new SystemNativeLoader(), CurrentPlatform, CurrentArchitecture) { }

    internal PlayerNativePlugins(string dataDirectory, string? substituteDirectory, Action<string> log,
        INativeLoader loader, NativePlatform platform, string architecture)
    {
        _data = dataDirectory;
        _substitutes = substituteDirectory;
        _log = log;
        _loader = loader;
        _platform = platform;
        _architecture = architecture;
    }

    internal static NativePlatform CurrentPlatform =>
        OperatingSystem.IsWindows() ? NativePlatform.Windows
        : OperatingSystem.IsMacOS() ? NativePlatform.MacOs
        : NativePlatform.Linux;

    /// <summary>The name of a <c>Plugins</c> subdirectory for the process architecture.</summary>
    internal static string CurrentArchitecture => RuntimeInformation.ProcessArchitecture switch
    {
        Architecture.X64 => "x86_64",
        Architecture.X86 => "x86",
        Architecture.Arm64 => "arm64",
        Architecture.Arm => "arm",
        var other => other.ToString().ToLowerInvariant(),
    };

    /// <summary>The file names a library called <paramref name="name"/> has on <paramref name="platform"/>, most specific first.</summary>
    internal static string[] FileNames(string name, NativePlatform platform) => platform switch
    {
        NativePlatform.Windows => new[] { name + ".dll" },
        NativePlatform.MacOs => new[] { name + ".bundle", "lib" + name + ".dylib", name + ".dylib" },
        _ => new[] { "lib" + name + ".so", name + ".so" },
    };

    /// <summary>Makes <paramref name="assembly"/>'s native imports resolve through this player.</summary>
    public void Register(Assembly assembly) =>
        System.Runtime.InteropServices.NativeLibrary.SetDllImportResolver(assembly, Resolve);

    /// <summary>The handle for <paramref name="name"/> as <paramref name="assembly"/> imports it, or zero.</summary>
    /// <exception cref="DllNotFoundException">A library file exists but does not load.</exception>
    internal nint Resolve(string name, Assembly assembly, DllImportSearchPath? searchPath)
    {
        // Default probing may consult this resolver again; the inner call answers "not found".
        if (_probing) return 0;
        nint probed;
        _probing = true;
        try { probed = _loader.Probe(name, assembly, searchPath); }
        finally { _probing = false; }
        if (probed != 0) return probed;
        lock (_gate)
        {
            if (_found.TryGetValue(name, out var known)) return known;
            var handle = FromPlugins(name);
            if (handle == 0) handle = FromSubstitute(name);
            if (handle != 0) _found[name] = handle;
            return handle;
        }
    }

    private nint FromPlugins(string name)
    {
        var plugins = Path.Combine(_data, "Plugins");
        foreach (var directory in new[] { Path.Combine(plugins, _architecture), plugins })
            foreach (var file in FileNames(name, _platform))
            {
                var path = Path.Combine(directory, file);
                if (File.Exists(path)) return Load(name, path);
            }
        return 0;
    }

    private nint FromSubstitute(string name)
    {
        if (_substitutes is null || !Substitutes.TryGetValue(name, out var substitute)) return 0;
        foreach (var file in FileNames(substitute, _platform))
        {
            var path = Path.Combine(_substitutes, file);
            if (!File.Exists(path)) continue;
            var handle = Load(name, path);
            _log($"native plugin {name}: the player ships none for this platform; using the engine's silent library {file}");
            return handle;
        }
        return 0;
    }

    private nint Load(string name, string path)
    {
        try { return _loader.Load(path); }
        catch (Exception error) when (error is DllNotFoundException or BadImageFormatException or FileNotFoundException)
        {
            throw new DllNotFoundException($"native plugin {name}: {path} exists but does not load: {error.Message}", error);
        }
    }
}
