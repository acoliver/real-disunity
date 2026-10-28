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
/// <c>{"player": data directory, "plan": path, "first_texture": 1}</c>. Each step writes a line
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

    private static void RunPhase(RuntimeHooks hooks, RuntimeInitializeLoadType phase)
    {
        Log($"startup phase {phase}");
        hooks.Run(phase);
    }

    private static byte[] NoFont(string name) =>
        throw new FileNotFoundException($"player plans name no package fonts, but the scene asked for {name}");

    private static void Log(string line) => Console.Error.WriteLine($"[player] {line}");
}
