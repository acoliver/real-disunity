using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine;

public class ScriptableObject : Object
{
    internal ulong Identity;
    internal ulong World;
    internal override GameObject Owner => throw new NotSupportedException("script assets have no GameObject");
    internal override unsafe bool Destroyed
    {
        get
        {
            if (World == 0 || World != DisunityHost.WorldHandle) return true;
            uint alive;
            DisunityHost.ThrowIfError(NativeBindings.disunity_asset_alive(World, Identity, &alive));
            return alive == 0;
        }
    }
    public static T CreateInstance<T>() where T : ScriptableObject => (T)CreateInstance(typeof(T));
    public static ScriptableObject CreateInstance(Type type)
    {
        ArgumentNullException.ThrowIfNull(type);
        if (!typeof(ScriptableObject).IsAssignableFrom(type) || type.IsAbstract || type.ContainsGenericParameters)
            throw new ArgumentException("type must be a concrete ScriptableObject", nameof(type));
        var asset = (ScriptableObject)Activator.CreateInstance(type, nonPublic: true)!;
        return DisunityHost.RegisterAsset(asset);
    }
    public static ScriptableObject CreateInstance(string className)
    {
        var type = AppDomain.CurrentDomain.GetAssemblies().SelectMany(a => a.GetTypes())
            .Single(t => t.FullName == className || t.Name == className);
        return CreateInstance(type);
    }
}
