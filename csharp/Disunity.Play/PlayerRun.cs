using System.Reflection;
using System.Runtime.Loader;
using System.Text.Json;
using Disunity.Hosting;
using UnityEngine;

namespace Disunity.Play;

/// <summary>
/// Starts a built Unity player the way its runtime does: the player's assemblies resolve in
/// the bridge's load context, its startup hooks run in Unity's phase order around the first
/// scene, and the scene loads from its plan. A player request is
/// <c>{"player": data directory, "plan": path, "resources": path, "identity": {...},
/// "persistent": directory, "arguments": [the player's command line after its executable path],
/// "first_texture": 1, "quality": {"names": [...], "current": 0},
/// "scenes": [{"name": ..., "path": ..., "plan": path or null, "error": why it has no plan}]}</c>, with the scene and
/// resource plans, the player identity and its quality levels (when it defines any)
/// <c>unity-import</c> writes. <c>scenes</c> lists the player's build in build order for <c>SceneManager</c>. Each step writes a line
/// to standard error, so the first one that fails is the last line written.
/// </summary>
internal static class PlayerRun
{
    public static LoadedScene Start(JsonElement request, AssemblyLoadContext context)
    {
        var data = request.GetProperty("player").GetString()!;
        var manifest = PlayerManifest.Read(data);
        Log($"player {data}: {manifest.GameAssemblies.Count} game assemblies, {manifest.Hooks.Count} startup hooks");
        var plugins = new PlayerNativePlugins(data, DisunityHost.NativeLibraryDirectory, Log);
        // A Mono player gets some class libraries from its runtime and does not ship them; the bridge carries them.
        var bridge = Path.GetDirectoryName(typeof(PlayerRun).Assembly.Location);
        var assemblies = new PlayerAssemblies(Path.Combine(data, "Managed"), typeof(DisunityHost).Assembly, manifest.GameAssemblies, plugins, bridge);
        assemblies.Attach(context);
        assemblies.ServeDefaultContext();
        // Unity's Mono runtime loads a player into one domain, so a lookup by name, such as
        // BinaryFormatter's or Type.GetType's from class-library code, finds the game's assemblies.
        // Entering the player's context makes those lookups resolve there for the rest of the run.
        context.EnterContextualReflection();
        Assembly Load(string name) => context.LoadFromAssemblyName(new AssemblyName(name));
        // A player's runtime loads every listed assembly at startup; packages that find their types by enumerating
        // the loaded assemblies (a pipeline's volume components, for one) rely on it.
        Log($"loaded {assemblies.LoadListed(Log)} of {manifest.GameAssemblies.Count} listed assemblies");
        // The screen is the window the frame is drawn into, so the game's canvases scale for the size they are drawn at.
        var (screenWidth, screenHeight) = PlayerScreen.Size(request);
        DisunityHost.SetScreen(screenWidth, screenHeight, 96f, 60f);
        var commandLine = PlayerCommandLine.Build(request);
        PlayerCommandLine.Install(commandLine);
        Log($"command line: {string.Join(' ', commandLine)}");
        DisunityHost.ConfigurePlayer(Environment(data, request));
        ApplyQuality(request);
        // The player runs without a window; it reports the renderer's guaranteed limits, not a device of zeros.
        DisunityHost.SetGraphicsDevice(GraphicsDeviceInfo.Windowless);
        DisunityHost.LoadResources(request.GetProperty("resources").GetString()!, Load);
        DisunityHost.ApplyRenderPipelineAssets(Log);
        DisunityHost.ApplyInputAxes(Log);
        var hooks = RuntimeHooks.Resolve(manifest.Hooks, Load);
        foreach (var unresolved in hooks.Unresolved) Log($"startup hook unresolved: {unresolved.Hook}: {unresolved.Reason}");
        var beforeScene = RuntimeHooks.PhaseOrder.Where(phase => phase != RuntimeInitializeLoadType.AfterSceneLoad);
        foreach (var phase in beforeScene) RunPhase(hooks, phase);
        // The pipeline package resets its static state in its startup hooks, so a pipeline made before they ran
        // would lose the volume system it set up. Unity makes it when it first renders, after the hooks.
        DisunityHost.StartRenderPipeline();
        // Player plans name each script's assembly, so the default scripts assembly is unused.
        var options = new SceneLoadOptions(typeof(PlayerRun).Assembly, NoFont)
        {
            FirstTextureIdentity = request.GetProperty("first_texture").GetUInt64(),
            Assemblies = Load,
        };
        if (request.TryGetProperty("scenes", out var scenes)) DisunityHost.SetBuildScenes(BuildScenes(scenes), options);
        var planPath = request.GetProperty("plan").GetString()!;
        Log($"loading scene {planPath}");
        var scene = DisunityHost.LoadScene(File.ReadAllText(planPath), options);
        try
        {
            foreach (var line in scene.Differences) Log($"not reproduced: {line}");
            RunPhase(hooks, RuntimeInitializeLoadType.AfterSceneLoad);
        }
        catch
        {
            scene.Dispose();
            throw;
        }
        Log("scene loaded");
        return scene;
    }

    /// <summary>Runs a startup phase as Unity does: a hook that fails is logged with
    /// <see cref="Debug.LogException(Exception)"/> and the phase goes on.</summary>
    private static void RunPhase(RuntimeHooks hooks, RuntimeInitializeLoadType phase)
    {
        Log($"startup phase {phase}");
        hooks.Run(phase, (hook, error) =>
        {
            Log($"startup hook failed: {hook}: {error.GetType().Name}: {error.Message}");
            Debug.LogException(error);
        });
    }

    /// <summary>The player's build scenes. A scene without a plan loads by throwing the planning error.</summary>
    private static List<BuildScene> BuildScenes(JsonElement scenes) => scenes.EnumerateArray().Select(scene =>
    {
        var name = scene.GetProperty("name").GetString()!;
        var plan = scene.GetProperty("plan").GetString();
        var error = scene.TryGetProperty("error", out var why) ? why.GetString() : null;
        return new BuildScene(name, scene.GetProperty("path").GetString()!,
            plan is null ? null : () => File.ReadAllText(plan), plan is null ? error ?? "it was not planned" : null);
    }).ToList();

    private static byte[] NoFont(string name) =>
        throw new FileNotFoundException($"player plans name no package fonts, but the scene asked for {name}");

    private static void Log(string line) => Console.Error.WriteLine($"[player] {line}");

    /// <summary>Gives the engine the player's quality levels. A current level outside the names fails the
    /// run, since the player's own data contradicts itself.</summary>
    private static void ApplyQuality(JsonElement request)
    {
        if (!request.TryGetProperty("quality", out var quality)) return;
        var names = quality.GetProperty("names").EnumerateArray().Select(name => name.GetString()!).ToArray();
        var current = quality.GetProperty("current").GetInt32();
        if (current < 0 || current >= names.Length)
            throw new InvalidOperationException($"the player's current quality level {current} is not one of its {names.Length} levels");
        DisunityHost.SetQualityLevels(names, current);
        Log($"quality levels: {string.Join(", ", names)} (current {names[current]})");
    }

    /// <summary>What <c>Application</c> reports, from the player's identity.</summary>
    private static PlayerEnvironment Environment(string data, JsonElement request)
    {
        var identity = request.GetProperty("identity");
        return new PlayerEnvironment(
            Path.GetFullPath(data).Replace('\\', '/').TrimEnd('/'),
            Path.GetFullPath(request.GetProperty("persistent").GetString()!).Replace('\\', '/'),
            identity.GetProperty("company_name").GetString()!,
            identity.GetProperty("product_name").GetString()!,
            identity.GetProperty("version").GetString()!,
            identity.GetProperty("unity_version").GetString()!,
            Enum.Parse<RuntimePlatform>(identity.GetProperty("platform").GetString()!));
    }
}
