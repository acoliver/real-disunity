using Disunity.Hosting;
using UnityEngine.Events;

namespace UnityEngine.SceneManagement;

/// <summary>
/// The scenes of the world. The engine holds one loaded scene, so there is at most one, and loading
/// another scene from a script is not supported yet: the engine cannot load another scene's plan
/// at run time. <see cref="sceneLoaded"/> and <see cref="sceneUnloaded"/> handlers last as long as the world.
/// </summary>
public static class SceneManager
{
    public static event UnityAction<Scene, LoadSceneMode>? sceneLoaded;

    public static event UnityAction<Scene>? sceneUnloaded;

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

    public static void LoadScene(string sceneName) => throw Unsupported(nameof(LoadScene), sceneName, LoadSceneMode.Single);

    public static void LoadScene(string sceneName, LoadSceneMode mode) => throw Unsupported(nameof(LoadScene), sceneName, mode);

    public static AsyncOperation LoadSceneAsync(string sceneName, LoadSceneMode mode) =>
        throw Unsupported(nameof(LoadSceneAsync), sceneName, mode);

    private static NotSupportedException Unsupported(string member, string sceneName, LoadSceneMode mode) =>
        new($"SceneManager.{member}(\"{sceneName}\", {mode}) is not supported yet: the engine holds one scene");

    internal static void RaiseLoaded(Scene scene) => Raise(sceneLoaded, handler => handler(scene, LoadSceneMode.Single));

    internal static void RaiseUnloaded(Scene scene) => Raise(sceneUnloaded, handler => handler(scene));

    internal static void ClearHandlers() => (sceneLoaded, sceneUnloaded) = (null, null);

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
