using Disunity.Hosting;

namespace UnityEngine.SceneManagement;

public readonly partial struct Scene
{
    /// <summary>The handle of the one loaded scene.</summary>
    internal const int LoadedHandle = 1;

    public int handle => Handle;
    public bool isLoaded => IsValid();
    public string name => IsValid() && DisunityHost.CurrentScene is { } info ? info.Name : "";
    public string path => "";

    /// <summary>The scene's index in the build: the number in a player's level name, 0 for a project plan, -1 for an invalid scene.</summary>
    public int buildIndex => IsValid() && DisunityHost.CurrentScene is { } info ? info.BuildIndex : -1;

    public int rootCount => IsValid() ? DisunityHost.SceneRoots().Count : 0;

    /// <summary>The scene's objects with no parent, in sibling order; empty for an invalid scene.</summary>
    public GameObject[] GetRootGameObjects() => IsValid() ? DisunityHost.SceneRoots().ToArray() : [];

    public void GetRootGameObjects(List<GameObject> rootGameObjects)
    {
        ArgumentNullException.ThrowIfNull(rootGameObjects);
        rootGameObjects.Clear();
        rootGameObjects.AddRange(GetRootGameObjects());
    }
}
