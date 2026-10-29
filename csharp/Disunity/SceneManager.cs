using Disunity.Hosting;
using UnityEngine.Events;

namespace UnityEngine.SceneManagement;

/// <summary>
/// The scenes of the world. The engine holds one loaded scene, so there is at most one. A script loads another scene
/// of the build by name, path or build index in <see cref="LoadSceneMode.Single"/> mode: the old scene is unloaded
/// and the new one loads in the delayed-tasks phase of the frame that is running or next runs (the engine's choice).
/// <see cref="LoadSceneMode.Additive"/> loads and unloading a scene are not supported. <see cref="sceneLoaded"/>,
/// <see cref="sceneUnloaded"/> and <see cref="activeSceneChanged"/> handlers last as long as the world.
/// </summary>
public static class SceneManager
{
    public static event UnityAction<Scene, LoadSceneMode>? sceneLoaded;

    public static event UnityAction<Scene>? sceneUnloaded;

    public static event UnityAction<Scene, Scene>? activeSceneChanged;

    /// <summary>The number of scenes in the build: a player's list, or one for a project run.</summary>
    public static int sceneCountInBuildSettings => DisunityHost.BuildScenes.Count;

    public static int sceneCount => DisunityHost.ActiveScene.IsValid() ? 1 : 0;

    /// <summary>The loaded scene; an invalid scene when none is loaded.</summary>
    public static Scene GetActiveScene() => DisunityHost.ActiveScene;

    public static Scene GetSceneAt(int index) =>
        index == 0 && DisunityHost.ActiveScene.IsValid()
            ? DisunityHost.ActiveScene
            : throw new ArgumentException($"scene index {index} is out of range; the engine holds {sceneCount} scenes", nameof(index));

    /// <summary>True for the loaded scene, false for a valid scene that is not loaded.</summary>
    public static bool SetActiveScene(Scene scene)
    {
        if (!scene.IsValid()) throw new ArgumentException("the scene is not valid", nameof(scene));
        return scene == DisunityHost.ActiveScene;
    }

    public static void LoadScene(string sceneName) => LoadScene(sceneName, LoadSceneMode.Single);

    public static void LoadScene(string sceneName, LoadSceneMode mode) => LoadSceneAsync(sceneName, mode);

    public static void LoadScene(int sceneBuildIndex) => LoadScene(sceneBuildIndex, LoadSceneMode.Single);

    public static void LoadScene(int sceneBuildIndex, LoadSceneMode mode) => LoadSceneAsync(sceneBuildIndex, mode);

    public static AsyncOperation LoadSceneAsync(string sceneName) => LoadSceneAsync(sceneName, LoadSceneMode.Single);

    public static AsyncOperation LoadSceneAsync(string sceneName, LoadSceneMode mode) =>
        DisunityHost.QueueSceneLoad(DisunityHost.ResolveBuildScene(sceneName), mode);

    public static AsyncOperation LoadSceneAsync(int sceneBuildIndex) => LoadSceneAsync(sceneBuildIndex, LoadSceneMode.Single);

    public static AsyncOperation LoadSceneAsync(int sceneBuildIndex, LoadSceneMode mode) =>
        DisunityHost.QueueSceneLoad(DisunityHost.ResolveBuildScene(sceneBuildIndex), mode);

    internal static void RaiseLoaded(Scene scene) => Raise(sceneLoaded, handler => handler(scene, LoadSceneMode.Single));

    internal static void RaiseUnloaded(Scene scene) => Raise(sceneUnloaded, handler => handler(scene));

    internal static void RaiseActiveSceneChanged(Scene from, Scene to) => Raise(activeSceneChanged, handler => handler(from, to));

    internal static void ClearHandlers() => (sceneLoaded, sceneUnloaded, activeSceneChanged) = (null, null, null);

    /// <summary>Calls each handler in turn. One that throws is reported as a script exception and the
    /// rest still run, so a game's bad handler cannot stop a scene from loading.</summary>
    private static void Raise<T>(T? handlers, Action<T> call) where T : Delegate
    {
        if (handlers is null) return;
        foreach (var handler in handlers.GetInvocationList().Cast<T>())
        {
            try { call(handler); }
            catch (Exception error) when (DisunityHost.IsGameException(error)) { DisunityHost.ReportScriptException(error, null); }
        }
    }
}

/// <summary>The paths and build indexes of the scenes in the build.</summary>
public static class SceneUtility
{
    /// <summary>The path of build scene <paramref name="buildIndex"/>; empty when the build has no such scene.</summary>
    public static string GetScenePathByBuildIndex(int buildIndex) =>
        buildIndex >= 0 && buildIndex < DisunityHost.BuildScenes.Count ? DisunityHost.BuildScenes[buildIndex].Path : "";

    /// <summary>The build index of the scene whose path is <paramref name="scenePath"/>, or -1.</summary>
    public static int GetBuildIndexByScenePath(string scenePath)
    {
        ArgumentNullException.ThrowIfNull(scenePath);
        return DisunityHost.FindBuildScene(scenePath);
    }
}
