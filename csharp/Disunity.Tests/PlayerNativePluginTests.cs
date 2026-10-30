using System.Reflection;
using System.Runtime.InteropServices;
using System.Runtime.Loader;
using Disunity.Hosting;

/// <summary>
/// A player's native plugin resolution: the search order for a library the runtime's default
/// probing does not find (the player's <c>Plugins</c> directories, then the engine's substitute,
/// then nothing), with a recording stand-in for the operating system's loader, and one real
/// <c>DllImport</c> into the silent Wwise library the engine library's directory holds.
/// </summary>
internal static class PlayerNativePluginTests
{
    private const string Wwise = "AkSoundEngine";
    private const string Arch = "x86_64";

    public static void Run()
    {
        FileNamesFollowThePlatform();
        foreach (var platform in Enum.GetValues<NativePlatform>()) SearchOrder(platform);
        DefaultProbingComesFirst();
        UnknownNamesStayUnresolved();
        ABrokenLibraryFileFailsWithItsPath();
        SubstituteNeedsItsDirectory();
        LoadedGameAssembliesGetTheResolver();
        Console.WriteLine("PASS: player native plugin search order, substitutes and failures");
    }

    public static void RunImport()
    {
        var substitutes = DisunityHost.NativeLibraryDirectory ?? throw new InvalidOperationException("the engine library is configured");
        var data = Directory.CreateTempSubdirectory("player-data");
        try
        {
            var lines = new List<string>();
            var plugins = new PlayerNativePlugins(data.FullName, substitutes, lines.Add);
            plugins.Register(typeof(PlayerNativePluginTests).Assembly);
            Require(Imports.CSharp_IsInitialized() == 0, "the silent Wwise library reports zero for IsInitialized");
            Require(Imports.CSharp_AK_INVALID_AUDIO_OBJECT_ID_get() == 0, "a ulong entry point reports zero");
            Imports.CSharp_AkAudioSettings_uNumSamplesPerFrame_set(0, 1024);
            Require(lines.Count == 1 && lines[0].Contains(Wwise) && lines[0].Contains("silent"),
                "one line names the library and that it is silent: " + string.Join("|", lines));
        }
        finally { data.Delete(recursive: true); }
        Console.WriteLine("PASS: a player assembly's AkSoundEngine imports bind to the silent Wwise library");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition) throw new InvalidOperationException(message);
    }

    private static void FileNamesFollowThePlatform()
    {
        Require(PlayerNativePlugins.FileNames("Plug", NativePlatform.Windows).SequenceEqual(new[] { "Plug.dll" }), "Windows names");
        Require(PlayerNativePlugins.FileNames("Plug", NativePlatform.Linux).SequenceEqual(new[] { "libPlug.so", "Plug.so" }), "Linux names");
        Require(PlayerNativePlugins.FileNames("Plug", NativePlatform.MacOs).SequenceEqual(new[] { "Plug.bundle", "libPlug.dylib", "Plug.dylib" }),
            "macOS names");
        Require(PlayerNativePlugins.Substitutes.Count == 1 && PlayerNativePlugins.Substitutes[Wwise] == "worldrunner_silent_wwise",
            "AkSoundEngine has the silent Wwise substitute");
    }

    private static void SearchOrder(NativePlatform platform)
    {
        using var scratch = new Scratch(platform);
        var arch = scratch.Plugin(Path.Combine("Plugins", Arch), Wwise);
        var flat = scratch.Plugin("Plugins", Wwise);
        var substitute = scratch.Substitute();
        var plugins = scratch.Plugins();
        var assembly = typeof(PlayerNativePluginTests).Assembly;
        Require(plugins.Resolve(Wwise, assembly, null) == scratch.Loader.Handle(arch), $"{platform}: Plugins/{Arch} comes first");
        File.Delete(arch);
        var second = scratch.Plugins();
        Require(second.Resolve(Wwise, assembly, null) == scratch.Loader.Handle(flat), $"{platform}: Plugins comes second");
        File.Delete(flat);
        var third = scratch.Plugins();
        Require(third.Resolve(Wwise, assembly, null) == scratch.Loader.Handle(substitute), $"{platform}: the substitute comes last");
        Require(third.Resolve(Wwise, assembly, null) == scratch.Loader.Handle(substitute), $"{platform}: the substitute stays resolved");
        Require(scratch.Lines.Count == 1 && scratch.Lines[0].Contains(Wwise) && scratch.Lines[0].Contains("silent"),
            $"{platform}: one log line per library: " + string.Join("|", scratch.Lines));
        Require(scratch.Loader.Loaded.Count(path => path == substitute) == 1, $"{platform}: the substitute loads once");
    }

    private static void DefaultProbingComesFirst()
    {
        using var scratch = new Scratch(PlayerNativePlugins.CurrentPlatform);
        scratch.Plugin("Plugins", Wwise);
        scratch.Substitute();
        scratch.Loader.Probed = 77;
        var handle = scratch.Plugins().Resolve(Wwise, typeof(PlayerNativePluginTests).Assembly, null);
        Require(handle == 77 && scratch.Loader.Loaded.Count == 0 && scratch.Lines.Count == 0,
            "a library default probing finds is used as it is, with no search and no log");
    }

    private static void UnknownNamesStayUnresolved()
    {
        using var scratch = new Scratch(PlayerNativePlugins.CurrentPlatform);
        scratch.Substitute();
        Require(scratch.Plugins().Resolve("NoSuchPlugin", typeof(PlayerNativePluginTests).Assembly, null) == 0,
            "a name with no plugin file and no substitute resolves to zero");
        Require(scratch.Loader.Loaded.Count == 0 && scratch.Lines.Count == 0, "nothing loads and nothing is logged for it");
    }

    private static void ABrokenLibraryFileFailsWithItsPath()
    {
        using var scratch = new Scratch(PlayerNativePlugins.CurrentPlatform);
        var broken = scratch.Plugin("Plugins", Wwise);
        scratch.Substitute();
        scratch.Loader.Broken.Add(broken);
        try
        {
            scratch.Plugins().Resolve(Wwise, typeof(PlayerNativePluginTests).Assembly, null);
            throw new InvalidOperationException("a plugin file that does not load must fail, not fall back to the substitute");
        }
        catch (DllNotFoundException error)
        {
            Require(error.Message.Contains(broken) && error.Message.Contains("bad image") && error.InnerException is BadImageFormatException,
                "the failure names the path and the cause: " + error.Message);
        }
    }

    private static void SubstituteNeedsItsDirectory()
    {
        using var scratch = new Scratch(PlayerNativePlugins.CurrentPlatform);
        scratch.Substitute();
        var none = new PlayerNativePlugins(scratch.Data, null, scratch.Lines.Add, scratch.Loader, scratch.Platform, Arch);
        Require(none.Resolve(Wwise, typeof(PlayerNativePluginTests).Assembly, null) == 0, "no substitute directory, no substitute");
    }

    private static void LoadedGameAssembliesGetTheResolver()
    {
        using var scratch = new Scratch(PlayerNativePlugins.CurrentPlatform);
        var managed = Path.Combine(scratch.Data, "Managed");
        Directory.CreateDirectory(managed);
        File.WriteAllBytes(Path.Combine(managed, "Game.dll"), new SyntheticAssembly("Game").Build());
        var context = new AssemblyLoadContext("native-plugins", isCollectible: true);
        try
        {
            var player = new PlayerAssemblies(managed, typeof(DisunityHost).Assembly, new[] { "Game" }, scratch.Plugins());
            player.Attach(context);
            var game = player.Resolve(context, new AssemblyName("Game")) ?? throw new InvalidOperationException("Game resolves");
            try
            {
                System.Runtime.InteropServices.NativeLibrary.SetDllImportResolver(game, (_, _, _) => 0);
                throw new InvalidOperationException("a game assembly the player loads must already have the player's native resolver");
            }
            catch (InvalidOperationException error) when (error.Message.Contains("already")) { }
        }
        finally { context.Unload(); }
    }

    /// <summary>The binding's declarations, as a game assembly would declare them.</summary>
    private static class Imports
    {
        [DllImport(Wwise)] internal static extern int CSharp_IsInitialized();
        [DllImport(Wwise)] internal static extern ulong CSharp_AK_INVALID_AUDIO_OBJECT_ID_get();
        [DllImport(Wwise)] internal static extern void CSharp_AkAudioSettings_uNumSamplesPerFrame_set(nint settings, uint value);
    }

    /// <summary>A loader that gives each path its own handle and fails for the paths marked broken.</summary>
    private sealed class RecordingLoader : INativeLoader
    {
        private readonly Dictionary<string, nint> _handles = new();
        public nint Probed;
        public List<string> Loaded { get; } = new();
        public HashSet<string> Broken { get; } = new();

        public nint Handle(string path) => _handles.TryGetValue(path, out var handle) ? handle : throw new InvalidOperationException($"{path} never loaded");

        public nint Probe(string name, Assembly assembly, DllImportSearchPath? searchPath) => Probed;

        public nint Load(string path)
        {
            if (Broken.Contains(path)) throw new BadImageFormatException("bad image");
            Loaded.Add(path);
            return _handles[path] = 1000 + _handles.Count;
        }
    }

    /// <summary>A temporary player data directory and substitute directory.</summary>
    private sealed class Scratch : IDisposable
    {
        private readonly DirectoryInfo _root = Directory.CreateTempSubdirectory("player-native");
        public NativePlatform Platform { get; }
        public RecordingLoader Loader { get; } = new();
        public List<string> Lines { get; } = new();
        public string Data => Path.Combine(_root.FullName, "Game_Data");
        private string Engine => Path.Combine(_root.FullName, "engine");

        public Scratch(NativePlatform platform) => Platform = platform;

        public PlayerNativePlugins Plugins() => new(Data, Engine, Lines.Add, Loader, Platform, Arch);

        /// <summary>Creates the platform's first file name for <paramref name="name"/> under <paramref name="relative"/>.</summary>
        public string Plugin(string relative, string name) => Create(Path.Combine(Data, relative), name);

        public string Substitute() => Create(Engine, PlayerNativePlugins.Substitutes[Wwise]);

        private string Create(string directory, string name)
        {
            Directory.CreateDirectory(directory);
            var path = Path.Combine(directory, PlayerNativePlugins.FileNames(name, Platform)[0]);
            File.WriteAllBytes(path, new byte[] { 0 });
            return path;
        }

        /// <summary>Removes the directories. Windows keeps a loaded assembly's file open until its
        /// context is collected, so a file that stays locked is reported and left to the temp cleaner.</summary>
        public void Dispose()
        {
            for (var attempt = 0; attempt < 10; attempt++)
            {
                try { _root.Delete(recursive: true); return; }
                catch (Exception error) when (error is IOException or UnauthorizedAccessException)
                {
                    GC.Collect();
                    GC.WaitForPendingFinalizers();
                    if (attempt == 9) Console.Error.WriteLine($"could not remove {_root.FullName}: {error.Message}");
                }
            }
        }
    }
}
