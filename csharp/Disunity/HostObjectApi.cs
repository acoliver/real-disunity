using System.Text;
using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    internal static unsafe string ObjectText(GameObject owner, uint field)
    {
        var bytes = new byte[1024];
        uint written;
        fixed (byte* target = bytes)
            ThrowIfError(NativeBindings.disunity_object_text_read(WorldHandle, owner.NativeHandle, field, target, (uint)bytes.Length, &written));
        return Encoding.UTF8.GetString(bytes, 0, checked((int)written));
    }
    internal static unsafe void ObjectText(GameObject owner, uint field, string text)
    {
        ArgumentNullException.ThrowIfNull(text);
        var bytes = Encoding.UTF8.GetBytes(text);
        fixed (byte* source = bytes)
            ThrowIfError(NativeBindings.disunity_object_text_write(WorldHandle, owner.NativeHandle, field, source, (uint)bytes.Length));
    }
    internal static unsafe uint ObjectFlags(GameObject owner)
    {
        uint flags;
        ThrowIfError(NativeBindings.disunity_object_flags(WorldHandle, owner.NativeHandle, &flags));
        return flags;
    }
    internal static IEnumerable<GameObject> ActiveObjects() => Tracked.Where(value => !value.IsDestroyed && value.activeInHierarchy);
    /// <summary>Active objects of <paramref name="type"/>: GameObjects, and the components of each active GameObject.</summary>
    internal static UnityEngine.Object[] FindObjects(Type type) => ActiveObjects()
        .SelectMany(value => (type.IsInstanceOfType(value) ? new UnityEngine.Object[] { value } : [])
            .Concat(Components(value).Where(type.IsInstanceOfType))).ToArray();
    internal static T[] FindObjects<T>() where T : UnityEngine.Object => Array.ConvertAll(FindObjects(typeof(T)), value => (T)value);
}
