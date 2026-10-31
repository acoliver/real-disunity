using Disunity.Hosting;

namespace UnityEngine.SceneManagement;

public readonly partial struct Scene
{
    /// <summary>The scene's handle; the default handle for an invalid scene. It is a <see cref="SceneHandle"/>.</summary>
    public SceneHandle handle => SceneHandle.Of(Handle);
    public bool isLoaded => IsValid() && DisunityHost.IsLoadedScene(Handle);
    public string name => DisunityHost.SceneInfo(Handle)?.Name ?? "";
    public string path => DisunityHost.SceneInfo(Handle)?.Path ?? "";

    /// <summary>The scene's place in the build list, or the number in a plan named like a player's level, or 0 for a project plan; -1 for a scene that is not loaded.</summary>
    public int buildIndex => DisunityHost.SceneInfo(Handle)?.BuildIndex ?? -1;

    public int rootCount => GetRootGameObjects().Length;

    /// <summary>The scene's objects with no parent, in sibling order; empty for a scene that is not loaded. Roots marked DontDestroyOnLoad are not the scene's.</summary>
    public GameObject[] GetRootGameObjects() =>
        IsValid() && DisunityHost.SceneInfo(Handle) is { } info && DisunityHost.IsLoadedScene(Handle) ? DisunityHost.RootsOf(info).ToArray() : [];

    public void GetRootGameObjects(List<GameObject> rootGameObjects)
    {
        ArgumentNullException.ThrowIfNull(rootGameObjects);
        rootGameObjects.Clear();
        rootGameObjects.AddRange(GetRootGameObjects());
    }
}
