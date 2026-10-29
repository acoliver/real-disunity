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
/// "persistent": directory, "first_texture": 1, "quality": {"names": [...], "current": 0}}</c>,
/// with the scene and resource plans, the player identity and its quality levels (when it
/// defines any) <c>unity-import</c> writes. Each step writes a line
/// to standard error, so the first one that fails is the last line written.
/// </summary>
internal static class PlayerRun
{
    public static LoadedScene Start(JsonElement request, AssemblyLoadContext context)
    {
        var data = request.GetProperty("player").GetString()!;
        var manifest = PlayerManifest.Read(data);
        Log($"player {data}: {manifest.GameAssemblies.Count} game assemblies, {manifest.Hooks.Count} startup hooks");
        new PlayerAssemblies(Path.Combine(data, "Managed"), typeof(DisunityHost).Assembly, manifest.GameAssemblies).Attach(context);
        Assembly Load(string name) => context.LoadFromAssemblyName(new AssemblyName(name));
        DisunityHost.ConfigurePlayer(Environment(data, request));
        ApplyQuality(request);
        DisunityHost.LoadResources(request.GetProperty("resources").GetString()!, Load);
        var hooks = RuntimeHooks.Resolve(manifest.Hooks, Load);
        foreach (var unresolved in hooks.Unresolved) Log($"startup hook unresolved: {unresolved.Hook}: {unresolved.Reason}");
        var beforeScene = RuntimeHooks.PhaseOrder.Where(phase => phase != RuntimeInitializeLoadType.AfterSceneLoad);
        foreach (var phase in beforeScene) RunPhase(hooks, phase);
        // Player plans name each script's assembly, so the default scripts assembly is unused.
        var options = new SceneLoadOptions(typeof(PlayerRun).Assembly, NoFont)
        {
            FirstTextureIdentity = request.GetProperty("first_texture").GetUInt64(),
            Assemblies = Load,
        };
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
