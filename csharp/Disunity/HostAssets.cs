using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly Dictionary<ulong, ScriptableObject> Assets = new();
    internal static unsafe ScriptableObject RegisterAsset(ScriptableObject asset)
    {
        EnsureWorld();
        var identity = checked(_nextScript++);
        ThrowIfError(NativeBindings.disunity_asset_create(WorldHandle, identity));
        asset.Identity = identity;
        asset.World = WorldHandle;
        Assets.Add(identity, asset);
        DisunityAdvance work;
        ThrowIfError(NativeBindings.disunity_asset_activate(WorldHandle, identity, &work));
        Drain(work);
        return asset;
    }
    internal static void DestroyAsset(ScriptableObject asset) =>
        ThrowIfError(NativeBindings.disunity_asset_destroy(WorldHandle, asset.Identity));

    private static void DispatchAsset(ulong identity, uint message)
    {
        var asset = Assets[identity];
        string name = message switch { 10 => "Awake", 11 => "OnEnable", 16 => "OnDisable", 17 => "OnDestroy", _ => throw new InvalidOperationException("invalid asset callback") };
        try { ScriptMessages.Invoke(asset, name); }
        finally { if (message == 17) Assets.Remove(identity); }
    }
}
