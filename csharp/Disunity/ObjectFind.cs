using Disunity.Hosting;

namespace UnityEngine;

/// <summary>
/// Object searches. The host finds the active GameObjects in the loaded world and their components,
/// so every overload that would include inactive objects throws <see cref="NotSupportedException"/>
/// instead of returning a result that leaves them out.
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

    /// <summary>Instance ID order is ascending, so the newest object comes first, as the IDs count down.</summary>
    private static Object[] Find(Type type, FindObjectsSortMode sortMode)
    {
        ArgumentNullException.ThrowIfNull(type);
        var found = DisunityHost.FindObjects(type);
        return sortMode == FindObjectsSortMode.InstanceID ? found.OrderBy(value => value._instanceId).ToArray() : found;
    }

    private static T[] Find<T>(FindObjectsSortMode sortMode) where T : Object =>
        Array.ConvertAll(Find(typeof(T), sortMode), value => (T)value);

    public static T[] FindObjectsByType<T>(FindObjectsSortMode sortMode) where T : Object => Find<T>(sortMode);

    public static T[] FindObjectsByType<T>(FindObjectsInactive findObjectsInactive, FindObjectsSortMode sortMode) where T : Object
    {
        RequireActiveOnly(findObjectsInactive, "FindObjectsByType");
        return Find<T>(sortMode);
    }

    public static T[] FindObjectsOfType<T>() where T : Object => Find<T>(FindObjectsSortMode.None);

    public static T[] FindObjectsOfType<T>(bool includeInactive) where T : Object
    {
        RequireActiveOnly(includeInactive, "FindObjectsOfType");
        return Find<T>(FindObjectsSortMode.None);
    }

    public static T? FindObjectOfType<T>() where T : Object => FindFirstObjectByType<T>();

    public static T? FindObjectOfType<T>(bool includeInactive) where T : Object
    {
        RequireActiveOnly(includeInactive, "FindObjectOfType");
        return FindFirstObjectByType<T>();
    }

    public static T? FindFirstObjectByType<T>() where T : Object => DisunityHost.FindObjects<T>().FirstOrDefault();

    public static T? FindFirstObjectByType<T>(FindObjectsInactive findObjectsInactive) where T : Object
    {
        RequireActiveOnly(findObjectsInactive, "FindFirstObjectByType");
        return FindFirstObjectByType<T>();
    }

    public static T? FindAnyObjectByType<T>() where T : Object => FindFirstObjectByType<T>();

    public static T? FindAnyObjectByType<T>(FindObjectsInactive findObjectsInactive) where T : Object
    {
        RequireActiveOnly(findObjectsInactive, "FindAnyObjectByType");
        return FindFirstObjectByType<T>();
    }

    public static Object[] FindObjectsByType(Type type, FindObjectsSortMode sortMode) => Find(type, sortMode);

    public static Object[] FindObjectsByType(Type type, FindObjectsInactive findObjectsInactive, FindObjectsSortMode sortMode)
    {
        RequireActiveOnly(findObjectsInactive, "FindObjectsByType");
        return Find(type, sortMode);
    }

    public static Object[] FindObjectsOfType(Type type) => Find(type, FindObjectsSortMode.None);

    public static Object[] FindObjectsOfType(Type type, bool includeInactive)
    {
        RequireActiveOnly(includeInactive, "FindObjectsOfType");
        return Find(type, FindObjectsSortMode.None);
    }

    [Obsolete("Use Object.FindObjectsByType instead.")]
    public static Object[] FindSceneObjectsOfType(Type type) => Find(type, FindObjectsSortMode.None);

    [Obsolete("Use Resources.FindObjectsOfTypeAll instead.")]
    public static Object[] FindObjectsOfTypeAll(Type type) =>
        throw new NotSupportedException("Object.FindObjectsOfTypeAll is not supported yet: the host cannot list inactive objects or assets");

    [Obsolete("Use Resources.FindObjectsOfTypeAll instead.")]
    public static Object[] FindObjectsOfTypeIncludingAssets(Type type) =>
        throw new NotSupportedException("Object.FindObjectsOfTypeIncludingAssets is not supported yet: the host cannot list assets");

    public static Object? FindObjectOfType(Type type) => FindFirstObjectByType(type);

    public static Object? FindObjectOfType(Type type, bool includeInactive)
    {
        RequireActiveOnly(includeInactive, "FindObjectOfType");
        return FindFirstObjectByType(type);
    }

    public static Object? FindFirstObjectByType(Type type) => Find(type, FindObjectsSortMode.None).FirstOrDefault();

    public static Object? FindFirstObjectByType(Type type, FindObjectsInactive findObjectsInactive)
    {
        RequireActiveOnly(findObjectsInactive, "FindFirstObjectByType");
        return FindFirstObjectByType(type);
    }

    public static Object? FindAnyObjectByType(Type type) => FindFirstObjectByType(type);

    public static Object? FindAnyObjectByType(Type type, FindObjectsInactive findObjectsInactive)
    {
        RequireActiveOnly(findObjectsInactive, "FindAnyObjectByType");
        return FindFirstObjectByType(type);
    }
}
