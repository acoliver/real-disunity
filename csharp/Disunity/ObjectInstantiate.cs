using Disunity.Hosting;

namespace UnityEngine;

/// <summary>
/// Cloning. A scene object is cloned by the native world. An asset is cloned by its own class and
/// the clone's name gets "(Clone)" appended, as Unity names a clone; a class that cannot copy itself
/// throws <see cref="NotSupportedException"/>.
/// </summary>
public partial class Object
{
    public static T Instantiate<T>(T original) where T : Object => Instantiate(original, null);

    public static T Instantiate<T>(T original, Transform? parent) where T : Object
    {
        ArgumentNullException.ThrowIfNull(original);
        if (original.Owner is null) return (T)original.CloneAsset();
        return DisunityHost.Instantiate(original, parent);
    }

    /// <summary>The clone keeps the parent's local position, which is what the native clone does; keeping
    /// the world position is not supported yet.</summary>
    public static T Instantiate<T>(T original, Transform? parent, bool worldPositionStays) where T : Object
    {
        ArgumentNullException.ThrowIfNull(original);
        if (worldPositionStays && parent is not null && original.Owner is not null)
            throw new NotSupportedException("Object.Instantiate with worldPositionStays is not supported yet");
        return Instantiate(original, parent);
    }

    public static T Instantiate<T>(T original, Vector3 position, Quaternion rotation) where T : Object =>
        Instantiate(original, position, rotation, null);

    public static T Instantiate<T>(T original, Vector3 position, Quaternion rotation, Transform? parent) where T : Object
    {
        ArgumentNullException.ThrowIfNull(original);
        if (original.Owner is null) return (T)original.CloneAsset();
        return DisunityHost.Instantiate(original, parent, position, rotation);
    }

    public static Object Instantiate(Object original) => Instantiate<Object>(original);

    public static Object Instantiate(Object original, Transform? parent) => Instantiate<Object>(original, parent);

    public static Object Instantiate(Object original, Transform? parent, bool instantiateInWorldSpace) =>
        Instantiate<Object>(original, parent, instantiateInWorldSpace);

    public static Object Instantiate(Object original, Vector3 position, Quaternion rotation) =>
        Instantiate<Object>(original, position, rotation);

    public static Object Instantiate(Object original, Vector3 position, Quaternion rotation, Transform? parent) =>
        Instantiate<Object>(original, position, rotation, parent);

    internal T Named<T>(T clone) where T : Object
    {
        clone.name = name + "(Clone)";
        clone.hideFlags = hideFlags;
        return clone;
    }
}
