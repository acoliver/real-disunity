using System.Text.RegularExpressions;
using UnityEngine;
using UnityEngine.SceneManagement;

namespace Disunity.Hosting;

/// <summary>The scene the loaded plan describes: its name and the plan's prefab templates, which belong to no scene.
/// <see cref="Entry"/> is the build scene it came from and <see cref="ListIndex"/> its place in the player's build list,
/// null for a plan outside any list.</summary>
internal sealed record LoadedSceneInfo(string Name, IReadOnlySet<GameObject> Templates, BuildScene Entry)
{
    public int Handle { get; init; }
    public int? ListIndex { get; init; }
    public LoadedScene? Loaded { get; init; }

    /// <summary>The scene's path in the build; empty for a plan outside a build list.</summary>
    public string Path => Entry.Path;

    /// <summary>The place in the build list; without a list, a player's levels are named level0, level1 and so on after their build index and any other plan is build index 0.</summary>
    public int BuildIndex => ListIndex ?? (Regex.Match(Name, @"^level(\d+)$") is { Success: true } match && int.TryParse(match.Groups[1].Value, out var index) ? index : 0);
}

public static partial class DisunityHost
{
    /// <summary>The scene the last <see cref="LoadScene"/> built, or null before one loads and after the world unloads.</summary>
    internal static LoadedSceneInfo? CurrentScene { get; private set; }

    /// <summary>The loaded scene, or an invalid scene when none is loaded.</summary>
    internal static Scene ActiveScene => CurrentScene is null ? default : new Scene { Handle = CurrentScene.Handle };

    private static int _sceneHandles;
    private static LoadedSceneInfo? _unloadingScene;

    /// <summary>What a scene handle names: the loaded scene, or the one a load is replacing while its events run.</summary>
    internal static LoadedSceneInfo? SceneInfo(int handle) =>
        CurrentScene is { } current && current.Handle == handle ? current
        : _unloadingScene is { } leaving && leaving.Handle == handle ? leaving : null;

    /// <summary>The loaded scene for an object that is alive, is not a prefab template and does not live under a
    /// DontDestroyOnLoad root; those belong to no scene, as the engine's choice.</summary>
    internal static Scene SceneOf(GameObject value) =>
        CurrentScene is { } info && !value.IsDestroyed && !info.Templates.Contains(value) && !value.transform.root.gameObject.PersistsAcrossLoads
            ? new Scene { Handle = info.Handle }
            : default;

    internal static List<GameObject> SceneRoots() =>
        CurrentScene is { } info
            ? ChildrenOf(0).Where(value => !info.Templates.Contains(value) && !value.PersistsAcrossLoads).ToList()
            : new List<GameObject>();

    /// <summary>Tells scripts the scene is going, while its objects still exist, then forgets the scene
    /// handlers, which last as long as the world.</summary>
    private static void UnloadScene()
    {
        if (CurrentScene is not null) SceneManager.RaiseUnloaded(ActiveScene);
        SceneManager.ClearHandlers();
        ResetBuildScenes();
    }
}
