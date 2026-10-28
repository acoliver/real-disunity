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

    public static Object? InstanceIDToObject(int instanceID) => ObjectRegistry.Find(instanceID);
    public static Object? EntityIdToObject(EntityId entityId) => ObjectRegistry.Find(entityId);
    public static bool InstanceIDIsValid(int instanceId) => ObjectRegistry.Find(instanceId) is not null;
    public static bool EntityIdIsValid(EntityId entityId) => ObjectRegistry.Find(entityId) is not null;

    /// <summary>
    /// Every loaded object of the type, inactive ones included, for GameObject, component and Object-wide questions about scene
    /// objects only. Assets cannot be listed, so a type that could include one (Object, or any asset type) throws
    /// <see cref="NotSupportedException"/> instead of a list that leaves them out.
    /// </summary>
    public static Object[] FindObjectsOfTypeAll(Type type)
    {
        ArgumentNullException.ThrowIfNull(type);
        if (type != typeof(GameObject) && !typeof(Component).IsAssignableFrom(type))
            throw new NotSupportedException($"Resources.FindObjectsOfTypeAll({type.Name}) is not supported yet: the engine lists scene objects (GameObject and components), not assets");
        return ObjectRegistry.SceneObjects(type);
    }

    public static T[] FindObjectsOfTypeAll<T>() where T : Object => Array.ConvertAll(FindObjectsOfTypeAll(typeof(T)), value => (T)value);

    /// <summary>The engine ships none of Unity's built-in resources (its default meshes, materials and fonts) yet.</summary>
    public static Object GetBuiltinResource(Type type, string path) =>
        throw new NotSupportedException($"Resources.GetBuiltinResource({type?.Name}, \"{path}\") is not supported yet: the engine has no built-in resources");

    public static T GetBuiltinResource<T>(string path) where T : Object => (T)GetBuiltinResource(typeof(T), path);
}
