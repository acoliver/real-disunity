using Disunity.Hosting;

namespace UnityEngine;

/// <summary>
/// Loads the assets of the player's resource folders by path, relative to a <c>Resources</c>
/// folder and without the extension. Paths are matched without regard to case. Without a
/// player's resources, nothing loads.
/// </summary>
public static class Resources
{
    public static Object? Load(string path) => Load(path, typeof(Object));
    public static T? Load<T>(string path) where T : Object => (T?)Load(path, typeof(T));
    public static Object? Load(string path, Type systemTypeInstance)
    {
        ArgumentNullException.ThrowIfNull(systemTypeInstance);
        return DisunityHost.Resources?.Load(path, systemTypeInstance);
    }

    public static Object[] LoadAll(string path) => LoadAll(path, typeof(Object));
    public static T[] LoadAll<T>(string path) where T : Object => LoadAll(path, typeof(T)).Cast<T>().ToArray();
    public static Object[] LoadAll(string path, Type systemTypeInstance)
    {
        ArgumentNullException.ThrowIfNull(systemTypeInstance);
        return DisunityHost.Resources?.LoadAll(path, systemTypeInstance) ?? [];
    }

    public static ResourceRequest LoadAsync(string path) => LoadAsync(path, typeof(Object));
    public static ResourceRequest LoadAsync<T>(string path) where T : Object => LoadAsync(path, typeof(T));
    public static ResourceRequest LoadAsync(string path, Type type) => new(() => Load(path, type));

    /// <summary>Loaded resources stay loaded for the world's lifetime, so there is nothing to unload.</summary>
    public static void UnloadAsset(Object assetToUnload) => ArgumentNullException.ThrowIfNull(assetToUnload);

    public static AsyncOperation UnloadUnusedAssets()
    {
        var operation = new AsyncOperation();
        operation.CompleteNextFrame();
        return operation;
    }
}
