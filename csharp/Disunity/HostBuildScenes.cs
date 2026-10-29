using UnityEngine;
using UnityEngine.SceneManagement;

namespace Disunity.Hosting;

/// <summary>
/// A scene of the build: its name, its path in the project (such as <c>Assets/Scenes/Menu.unity</c>), and how to read its
/// plan. A scene that failed to plan has no <see cref="ReadPlan"/> and says why in <see cref="PlanError"/>.
/// </summary>
public sealed record BuildScene(string Name, string Path, Func<string>? ReadPlan = null, string? PlanError = null);

public static partial class DisunityHost
{
    private static IReadOnlyList<BuildScene>? _buildScenes;
    private static SceneLoadOptions? _sceneOptions;

    /// <summary>
    /// Gives the world the scenes of a player's build, in build order, and the options every scene loads with. Call it
    /// before the first scene loads; a plan named <c>level{i}</c> is then build scene <c>i</c>. The list lasts until the
    /// world unloads.
    /// </summary>
    public static void SetBuildScenes(IReadOnlyList<BuildScene> scenes, SceneLoadOptions options)
    {
        ArgumentNullException.ThrowIfNull(scenes);
        ArgumentNullException.ThrowIfNull(options);
        EnsureWorld();
        _buildScenes = scenes.ToArray();
        _sceneOptions = options;
    }

    /// <summary>The build's scenes: the player's list, or for a project run just the plan it loaded.</summary>
    internal static IReadOnlyList<BuildScene> BuildScenes =>
        _buildScenes ?? (CurrentScene is { } info ? [info.Entry] : []);

    /// <summary>The build index of the scene a key names: its name, its path, or its path without <c>Assets/</c> and
    /// <c>.unity</c> (the engine's choice to accept all three); the first in build order wins. -1 when none matches.</summary>
    internal static int FindBuildScene(string key)
    {
        var scenes = BuildScenes;
        for (var i = 0; i < scenes.Count; i++)
            if (scenes[i].Name == key || scenes[i].Path == key || Shortened(scenes[i].Path) == key) return i;
        return -1;
    }

    private static string Shortened(string path)
    {
        var start = path.StartsWith("Assets/", StringComparison.Ordinal) ? "Assets/".Length : 0;
        var end = path.EndsWith(".unity", StringComparison.Ordinal) ? path.Length - ".unity".Length : path.Length;
        return path[start..Math.Max(start, end)];
    }

    internal static int ResolveBuildScene(string key)
    {
        ArgumentNullException.ThrowIfNull(key);
        var index = FindBuildScene(key);
        return index >= 0 ? index : throw new ArgumentException(
            $"scene \"{key}\" is not in the build; the build holds {BuildScenes.Count} scenes", nameof(key));
    }

    internal static int ResolveBuildScene(int index) =>
        index >= 0 && index < BuildScenes.Count ? index : throw new ArgumentException(
            $"scene build index {index} is out of range; the build holds {BuildScenes.Count} scenes", nameof(index));

    private static void ResetBuildScenes()
    {
        _buildScenes = null;
        _sceneOptions = null;
        PendingLoads.Clear();
        _pumpPosted = false;
        _sceneHandles = 0;
        _unloadingScene = null;
    }
}
