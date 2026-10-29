using Disunity.Hosting;

namespace UnityEngine.SceneManagement;

public readonly partial struct Scene
{
    public int handle => Handle;
    public bool isLoaded => IsValid() && DisunityHost.CurrentScene?.Handle == Handle;
    public string name => DisunityHost.SceneInfo(Handle)?.Name ?? "";
    public string path => DisunityHost.SceneInfo(Handle)?.Path ?? "";

    /// <summary>The scene's place in the build list, or the number in a plan named like a player's level, or 0 for a project plan; -1 for a scene that is not loaded.</summary>
    public int buildIndex => DisunityHost.SceneInfo(Handle)?.BuildIndex ?? -1;

    public int rootCount => isLoaded ? DisunityHost.SceneRoots().Count : 0;

    /// <summary>The scene's objects with no parent, in sibling order; empty for a scene that is not loaded. Roots marked DontDestroyOnLoad are not the scene's.</summary>
    public GameObject[] GetRootGameObjects() => isLoaded ? DisunityHost.SceneRoots().ToArray() : [];

    public void GetRootGameObjects(List<GameObject> rootGameObjects)
    {
        ArgumentNullException.ThrowIfNull(rootGameObjects);
        rootGameObjects.Clear();
        rootGameObjects.AddRange(GetRootGameObjects());
    }
}
