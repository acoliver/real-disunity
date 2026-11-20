using Disunity.Hosting;
using Disunity.Native;

namespace UnityEngine.Rendering;

/// <summary>
/// The shadow casters of the world, each renderer as its world box, shadow casting mode and layer, gathered once and shared by
/// every <see cref="ScriptableRenderContext.Cull"/> that asks until they can have changed. A pipeline culls many times in a
/// frame (one per camera, light and probe), and each cull would otherwise read the bounds of every renderer again. The
/// gather is dropped when the world's frame number changes, when a renderer is added or destroyed (DestroyImmediate included), when the world unloads
/// and at the end of a pipeline's render. A renderer that moves or is enabled between two culls of one render keeps the box it
/// had at the gather until then, which is the granularity the engine offers: a pipeline sets its casters up once per render.
/// </summary>
internal static class ShadowCasterCache
{
    private readonly record struct Stamp(ulong World, uint Frame, int Renderers, int Destructions);

    private static Stamp _stamp;
    private static DisunityShadowCaster[]? _casters;

    /// <summary>How many times the renderers were gathered; tests read it.</summary>
    internal static int Gathers { get; private set; }

    /// <summary>The casters of the world as of this frame, gathered if no shared gather is current.</summary>
    public static DisunityShadowCaster[] Get()
    {
        var stamp = new Stamp(DisunityHost.WorldHandle, Time.Frame, DisunityHost.NativeComponentVersion<Renderer>(),
            DisunityHost.ImmediateDestructions);
        if (_casters is null || stamp != _stamp)
        {
            _casters = Gather();
            _stamp = stamp;
        }
        return _casters;
    }

    /// <summary>Forgets the gathered casters; the next shadow query gathers again.</summary>
    public static void Invalidate() => _casters = null;

    private static DisunityShadowCaster[] Gather()
    {
        Gathers++;
        var found = new List<DisunityShadowCaster>();
        foreach (var renderer in DisunityHost.NativeComponentsOf<Renderer>())
        {
            if (!renderer || !renderer.enabled || !renderer.gameObject.activeInHierarchy) continue;
            var bounds = renderer.bounds;
            if (bounds.size == Vector3.zero) continue;
            found.Add(new DisunityShadowCaster
            {
                min_x = bounds.min.x, min_y = bounds.min.y, min_z = bounds.min.z,
                max_x = bounds.max.x, max_y = bounds.max.y, max_z = bounds.max.z,
                casting_mode = (uint)renderer.shadowCastingMode,
                layer = (uint)renderer.gameObject.layer,
            });
        }
        return found.ToArray();
    }
}
