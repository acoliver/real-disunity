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

    /// <summary>The roots this scene owns while it is not the active scene: those its additive load created, or
    /// those it held when another scene became active. The active scene owns every other root.</summary>
    public HashSet<GameObject> Roots { get; set; } = new();

    /// <summary>The scene's path in the build; empty for a plan outside a build list.</summary>
    public string Path => Entry.Path;

    /// <summary>The place in the build list; without a list, a player's levels are named level0, level1 and so on after their build index and any other plan is build index 0.</summary>
    public int BuildIndex => ListIndex ?? (Regex.Match(Name, @"^level(\d+)$") is { Success: true } match && int.TryParse(match.Groups[1].Value, out var index) ? index : 0);
}

public static partial class DisunityHost
{
    /// <summary>The loaded scenes in load order. <see cref="CurrentScene"/> is the active one.</summary>
    internal static List<LoadedSceneInfo> LoadedScenes { get; } = new();

    /// <summary>The active scene, or null before one loads and after the world unloads.</summary>
    internal static LoadedSceneInfo? CurrentScene { get; private set; }

    /// <summary>The active scene, or an invalid scene when none is loaded.</summary>
    internal static Scene ActiveScene => CurrentScene is null ? default : new Scene { Handle = CurrentScene.Handle };

    private static int _sceneHandles;
    private static readonly List<LoadedSceneInfo> UnloadingScenes = new();

    /// <summary>What a scene handle names: a loaded scene, or one a load is unloading while its events run.</summary>
    internal static LoadedSceneInfo? SceneInfo(int handle) =>
        LoadedScenes.Find(info => info.Handle == handle) ?? UnloadingScenes.Find(info => info.Handle == handle);

    internal static bool IsLoadedScene(int handle) => LoadedScenes.Exists(info => info.Handle == handle);

    /// <summary>The scene of an object that is alive, is not a prefab template and does not live under a
    /// DontDestroyOnLoad root; those belong to no scene, as the engine's choice. A root another loaded scene owns
    /// belongs to it; every other root belongs to the active scene, so objects created at run time join it.</summary>
    internal static Scene SceneOf(GameObject value)
    {
        if (CurrentScene is null || value.IsDestroyed || IsTemplate(value)) return default;
        var root = value.transform.root.gameObject;
        if (root.PersistsAcrossLoads) return default;
        var owner = LoadedScenes.Find(info => info != CurrentScene && info.Roots.Contains(root)) ?? CurrentScene;
        return new Scene { Handle = owner.Handle };
    }

    internal static List<GameObject> SceneRoots() => CurrentScene is { } info ? RootsOf(info) : new List<GameObject>();

    /// <summary>The live roots of a loaded scene.</summary>
    internal static List<GameObject> RootsOf(LoadedSceneInfo scene)
    {
        if (scene != CurrentScene)
            return scene.Roots.Where(root => !root.IsDestroyed && root.transform.parent is null).ToList();
        return ChildrenOf(0).Where(value => !IsTemplate(value) && !value.PersistsAcrossLoads
            && !LoadedScenes.Exists(info => info != scene && info.Roots.Contains(value))).ToList();
    }

    private static bool IsTemplate(GameObject value) => LoadedScenes.Exists(info => info.Templates.Contains(value));

    /// <summary>Makes a loaded scene the active one. The scene that was active keeps the roots it holds now.</summary>
    internal static bool ActivateScene(int handle)
    {
        var next = LoadedScenes.Find(info => info.Handle == handle);
        if (next is null) return false;
        if (next == CurrentScene) return true;
        var previous = ActiveScene;
        if (CurrentScene is { } old) old.Roots = RootsOf(old).ToHashSet();
        CurrentScene = next;
        SceneManager.RaiseActiveSceneChanged(previous, ActiveScene);
        return true;
    }

    /// <summary>Tells scripts the scene is going, while its objects still exist, then forgets the scene
    /// handlers, which last as long as the world.</summary>
    private static void UnloadScene()
    {
        foreach (var info in LoadedScenes.ToArray()) SceneManager.RaiseUnloaded(new Scene { Handle = info.Handle });
        LoadedScenes.Clear();
        SceneManager.ClearHandlers();
        Canvas.ClearHandlers();
        ResetBuildScenes();
    }
}
