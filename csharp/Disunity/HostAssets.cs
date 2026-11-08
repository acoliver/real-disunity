using Disunity.Native;
using UnityEngine;

namespace Disunity.Hosting;

public static partial class DisunityHost
{
    private static readonly Dictionary<ulong, ScriptableObject> Assets = new();

    /// <summary>The player's resources, which <see cref="UnityEngine.Resources"/> loads from.</summary>
    internal static ResourceDatabase? Resources { get; private set; }

    /// <summary>
    /// Makes <see cref="UnityEngine.Resources"/> load from the resource plan at
    /// <paramref name="planPath"/>. <paramref name="assemblies"/> returns the player assembly
    /// with a given name, for script assets. Loaded assets belong to the loaded world.
    /// </summary>
    public static void LoadResources(string planPath, Func<string, System.Reflection.Assembly> assemblies) =>
        Resources = new ResourceDatabase(planPath, assemblies);

    /// <summary>
    /// Builds the render pipeline assets the loaded resource plan names and sets them in
    /// <see cref="UnityEngine.Rendering.GraphicsSettings"/> and <see cref="UnityEngine.QualitySettings"/>, for a player
    /// to call before its first scene loads. An asset that cannot be built is reported to <paramref name="log"/> and
    /// left null. The active asset's pipeline is created here, so it exists when the first scene's components wake
    /// (the engine's choice). A player that runs startup hooks which reset the pipeline package's static state calls
    /// <see cref="ApplyRenderPipelineAssets"/> before them and <see cref="StartRenderPipeline"/> after them instead.
    /// </summary>
    public static void ApplyRenderPipelines(Action<string> log)
    {
        ApplyRenderPipelineAssets(log);
        StartRenderPipeline();
    }

    /// <summary>Sets the assets as <see cref="ApplyRenderPipelines"/> does and creates no pipeline.</summary>
    public static void ApplyRenderPipelineAssets(Action<string> log) => Resources?.ApplyGraphics(log);

    /// <summary>Creates the active asset's pipeline now if it has none, as a frame step would, so it exists when the first
    /// scene's components wake. A pipeline that exists is kept.</summary>
    public static void StartRenderPipeline()
    {
        if (IsLoaded) UpdateRenderPipeline();
    }

    /// <summary>
    /// Registers the input axes the loaded resource plan carries, which a player takes from its input manager, for a
    /// player to call before its first scene loads. An axis the engine cannot deliver is reported to
    /// <paramref name="log"/> and left unregistered, so reading it throws as an unknown axis does.
    /// </summary>
    public static void ApplyInputAxes(Action<string> log)
    {
        if (Resources is not { } resources) return;
        EnsureWorld();
        resources.ApplyInputAxes(log);
    }

    /// <summary>Stops <see cref="UnityEngine.Resources"/> loading from a player's resources.</summary>
    public static void UnloadResources() => Resources = null;
    internal static ScriptableObject RegisterAsset(ScriptableObject asset)
    {
        CreateAsset(asset);
        ActivateAsset(asset);
        return asset;
    }

    /// <summary>Makes the asset exist in the world, so it no longer compares equal to null, without running its
    /// <c>Awake</c> or <c>OnEnable</c>. Assets loaded together are all created before the first one activates.</summary>
    internal static unsafe void CreateAsset(ScriptableObject asset)
    {
        EnsureWorld();
        var identity = checked(_nextScript++);
        ThrowIfError(NativeBindings.disunity_asset_create(WorldHandle, identity));
        asset.Identity = identity;
        asset.World = WorldHandle;
        Assets.Add(identity, asset);
    }

    /// <summary>Runs the <c>Awake</c> and <c>OnEnable</c> of an asset <see cref="CreateAsset"/> made.</summary>
    internal static unsafe void ActivateAsset(ScriptableObject asset)
    {
        DisunityAdvance work;
        ThrowIfError(NativeBindings.disunity_asset_activate(WorldHandle, asset.Identity, &work));
        Drain(work);
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
