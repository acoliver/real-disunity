using System.Collections;
using System.Reflection;
using System.Runtime.CompilerServices;

namespace Disunity.Hosting;

/// <summary>
/// Creates the empty object loaded or copied data is bound to. A <c>[Serializable]</c> class with
/// no parameterless constructor, public or not, is created without running any constructor, so its
/// state is exactly its serialized data. Any other type is created with its parameterless
/// constructor, which runs its field initializers.
/// </summary>
internal static class SerializableInstance
{
    private const BindingFlags AnyInstance = BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic;

    /// <summary>
    /// Whether a value of the type is loaded and copied by its serialized fields: a <c>[Serializable]</c>
    /// type that is not a <c>UnityEngine.Object</c> or a delegate. Of the collections only arrays and
    /// <c>List&lt;T&gt;</c> are serialized as sequences; any other collection type is serialized by its own
    /// serialized fields, as a class deriving from <c>Dictionary</c> with key and value lists is, and a
    /// collection with none cannot be loaded.
    /// </summary>
    public static bool IsSerializedClass(Type type) =>
        type.IsDefined(typeof(SerializableAttribute)) && !typeof(UnityEngine.Object).IsAssignableFrom(type)
        && !typeof(Delegate).IsAssignableFrom(type)
        && (!typeof(IEnumerable).IsAssignableFrom(type) || DisunityHost.SerializedFields(type).Any());

    public static object Create(Type type) => NeedsNoConstructor(type)
        ? RuntimeHelpers.GetUninitializedObject(type)
        : Activator.CreateInstance(type, nonPublic: true) ?? throw new NotSupportedException($"cannot construct {type}");

    private static bool NeedsNoConstructor(Type type) =>
        type.IsClass && type.IsDefined(typeof(SerializableAttribute), inherit: false)
        && type.GetConstructor(AnyInstance, Type.EmptyTypes) is null;
}
