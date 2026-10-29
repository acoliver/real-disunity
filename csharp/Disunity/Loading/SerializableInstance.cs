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

    public static object Create(Type type) => NeedsNoConstructor(type)
        ? RuntimeHelpers.GetUninitializedObject(type)
        : Activator.CreateInstance(type, nonPublic: true) ?? throw new NotSupportedException($"cannot construct {type}");

    private static bool NeedsNoConstructor(Type type) =>
        type.IsClass && type.IsDefined(typeof(SerializableAttribute), inherit: false)
        && type.GetConstructor(AnyInstance, Type.EmptyTypes) is null;
}
