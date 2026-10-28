using Disunity.Hosting;

namespace UnityEngine;

/// <summary>
/// Object searches. The host finds the active GameObjects in the loaded world and their components,
/// so a search finds scene objects only: a search for any other kind of object (Material, Texture2D,
/// ScriptableObject, Shader, ...) throws <see cref="NotSupportedException"/> naming the member, as does
/// every overload that would include inactive objects, instead of returning a result that leaves
/// objects out. Searching for <see cref="Object"/> itself finds scene objects.
/// </summary>
public partial class Object
{
    private static void RequireActiveOnly(FindObjectsInactive inactive, string member)
    {
        if (inactive != FindObjectsInactive.Exclude)
            throw new NotSupportedException($"Object.{member} with FindObjectsInactive.Include is not supported yet: the host finds active objects only");
    }

    private static void RequireActiveOnly(bool includeInactive, string member) =>
        RequireActiveOnly(includeInactive ? FindObjectsInactive.Include : FindObjectsInactive.Exclude, member);

    private static void RequireSceneType(Type type, string member)
    {
        ArgumentNullException.ThrowIfNull(type);
        if (type == typeof(Object) || type == typeof(GameObject) || typeof(Component).IsAssignableFrom(type)) return;
        throw new NotSupportedException($"Object.{member} for {type.Name} is not supported yet: the host finds scene objects only (GameObject and components), not assets");
    }

    /// <summary>Instance ID order is ascending, so the newest object comes first, as the IDs count down.</summary>
    private static Object[] Find(Type type, FindObjectsSortMode sortMode, string member)
    {
        RequireSceneType(type, member);
        var found = DisunityHost.FindObjects(type);
        return sortMode == FindObjectsSortMode.InstanceID ? found.OrderBy(value => value._instanceId).ToArray() : found;
    }

    private static T[] Find<T>(FindObjectsSortMode sortMode, string member) where T : Object =>
        Array.ConvertAll(Find(typeof(T), sortMode, member), value => (T)value);

    private static T? First<T>(string member) where T : Object
    {
        RequireSceneType(typeof(T), member);
        return DisunityHost.FindObjects<T>().FirstOrDefault();
    }

    private static Object? First(Type type, string member) => Find(type, FindObjectsSortMode.None, member).FirstOrDefault();

    public static T[] FindObjectsByType<T>(FindObjectsSortMode sortMode) where T : Object => Find<T>(sortMode, nameof(FindObjectsByType));

    public static T[] FindObjectsByType<T>(FindObjectsInactive findObjectsInactive, FindObjectsSortMode sortMode) where T : Object
    {
        RequireActiveOnly(findObjectsInactive, nameof(FindObjectsByType));
        return Find<T>(sortMode, nameof(FindObjectsByType));
    }

    public static T[] FindObjectsOfType<T>() where T : Object => Find<T>(FindObjectsSortMode.None, nameof(FindObjectsOfType));

    public static T[] FindObjectsOfType<T>(bool includeInactive) where T : Object
    {
        RequireActiveOnly(includeInactive, nameof(FindObjectsOfType));
        return Find<T>(FindObjectsSortMode.None, nameof(FindObjectsOfType));
    }

    public static T? FindObjectOfType<T>() where T : Object => First<T>(nameof(FindObjectOfType));

    public static T? FindObjectOfType<T>(bool includeInactive) where T : Object
    {
        RequireActiveOnly(includeInactive, nameof(FindObjectOfType));
        return First<T>(nameof(FindObjectOfType));
    }

    public static T? FindFirstObjectByType<T>() where T : Object => First<T>(nameof(FindFirstObjectByType));

    public static T? FindFirstObjectByType<T>(FindObjectsInactive findObjectsInactive) where T : Object
    {
        RequireActiveOnly(findObjectsInactive, nameof(FindFirstObjectByType));
        return First<T>(nameof(FindFirstObjectByType));
    }

    public static T? FindAnyObjectByType<T>() where T : Object => First<T>(nameof(FindAnyObjectByType));

    public static T? FindAnyObjectByType<T>(FindObjectsInactive findObjectsInactive) where T : Object
    {
        RequireActiveOnly(findObjectsInactive, nameof(FindAnyObjectByType));
        return First<T>(nameof(FindAnyObjectByType));
    }

    public static Object[] FindObjectsByType(Type type, FindObjectsSortMode sortMode) => Find(type, sortMode, nameof(FindObjectsByType));

    public static Object[] FindObjectsByType(Type type, FindObjectsInactive findObjectsInactive, FindObjectsSortMode sortMode)
    {
        RequireActiveOnly(findObjectsInactive, nameof(FindObjectsByType));
        return Find(type, sortMode, nameof(FindObjectsByType));
    }

    public static Object[] FindObjectsOfType(Type type) => Find(type, FindObjectsSortMode.None, nameof(FindObjectsOfType));

    public static Object[] FindObjectsOfType(Type type, bool includeInactive)
    {
        RequireActiveOnly(includeInactive, nameof(FindObjectsOfType));
        return Find(type, FindObjectsSortMode.None, nameof(FindObjectsOfType));
    }

    [Obsolete("Use Object.FindObjectsByType instead.")]
    public static Object[] FindSceneObjectsOfType(Type type) => Find(type, FindObjectsSortMode.None, nameof(FindSceneObjectsOfType));

    [Obsolete("Use Resources.FindObjectsOfTypeAll instead.")]
    public static Object[] FindObjectsOfTypeAll(Type type) =>
        throw new NotSupportedException("Object.FindObjectsOfTypeAll is not supported yet: the host cannot list inactive objects or assets");

    [Obsolete("Use Resources.FindObjectsOfTypeAll instead.")]
    public static Object[] FindObjectsOfTypeIncludingAssets(Type type) =>
        throw new NotSupportedException("Object.FindObjectsOfTypeIncludingAssets is not supported yet: the host cannot list assets");

    public static Object? FindObjectOfType(Type type) => First(type, nameof(FindObjectOfType));

    public static Object? FindObjectOfType(Type type, bool includeInactive)
    {
        RequireActiveOnly(includeInactive, nameof(FindObjectOfType));
        return First(type, nameof(FindObjectOfType));
    }

    public static Object? FindFirstObjectByType(Type type) => First(type, nameof(FindFirstObjectByType));

    public static Object? FindFirstObjectByType(Type type, FindObjectsInactive findObjectsInactive)
    {
        RequireActiveOnly(findObjectsInactive, nameof(FindFirstObjectByType));
        return First(type, nameof(FindFirstObjectByType));
    }

    public static Object? FindAnyObjectByType(Type type) => First(type, nameof(FindAnyObjectByType));

    public static Object? FindAnyObjectByType(Type type, FindObjectsInactive findObjectsInactive)
    {
        RequireActiveOnly(findObjectsInactive, nameof(FindAnyObjectByType));
        return First(type, nameof(FindAnyObjectByType));
    }
}
