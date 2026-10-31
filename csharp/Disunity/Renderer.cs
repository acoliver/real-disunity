using Disunity.Native;
using UnityEngine.Rendering;

namespace UnityEngine;

/// <summary>
/// The base of every component that draws: <see cref="SpriteRenderer"/>, <see cref="MeshRenderer"/> and
/// <see cref="SkinnedMeshRenderer"/>. A renderer has its own <see cref="enabled"/> and is not a
/// <see cref="Behaviour"/>. The settings are held by the native component, so a clone from <c>Instantiate</c> has
/// its own. Materials, property blocks and the per-renderer material instances stay in managed memory, see
/// <see cref="sharedMaterials"/>. Nothing draws mesh renderers yet, so <see cref="isVisible"/> is always false.
/// A sprite renderer keeps the settings its native sprite does not hold (shadow, probe and priority settings) on
/// the managed component, where nothing reads them; their defaults are the ones a sprite has: no shadows, no probes.
/// The only sorting layer the engine registers is Default, id 0.
/// </summary>
public abstract partial class Renderer : NativeComponent
{
    private const int NoMaterialIndex = -1;

    internal abstract DisunityMeshRenderer ReadSettings();
    internal abstract void WriteSettings(DisunityMeshRenderer value);

    private void Update(Func<DisunityMeshRenderer, DisunityMeshRenderer> change) => WriteSettings(change(ReadSettings()));

    public bool enabled
    {
        get => ReadSettings().enabled == 1;
        set => Update(r => { r.enabled = value ? 1u : 0u; return r; });
    }

    /// <summary>Always false: nothing draws yet, so nothing is ever seen by a camera.</summary>
    public bool isVisible => false;

    /// <summary>Skips drawing without touching <see cref="enabled"/>.</summary>
    public bool forceRenderingOff
    {
        get => ReadSettings().force_rendering_off == 1;
        set => Update(r => { r.force_rendering_off = value ? 1u : 0u; return r; });
    }

    public ShadowCastingMode shadowCastingMode
    {
        get => (ShadowCastingMode)ReadSettings().shadow_casting_mode;
        set => Update(r => { r.shadow_casting_mode = unchecked((uint)(int)value); return r; });
    }

    public bool receiveShadows
    {
        get => ReadSettings().receive_shadows == 1;
        set => Update(r => { r.receive_shadows = value ? 1u : 0u; return r; });
    }

    public MotionVectorGenerationMode motionVectorGenerationMode
    {
        get => (MotionVectorGenerationMode)ReadSettings().motion_vector_mode;
        set => Update(r => { r.motion_vector_mode = unchecked((uint)(int)value); return r; });
    }

    public LightProbeUsage lightProbeUsage
    {
        get => (LightProbeUsage)ReadSettings().light_probe_usage;
        set => Update(r => { r.light_probe_usage = unchecked((uint)(int)value); return r; });
    }

    public ReflectionProbeUsage reflectionProbeUsage
    {
        get => (ReflectionProbeUsage)ReadSettings().reflection_probe_usage;
        set => Update(r => { r.reflection_probe_usage = unchecked((uint)(int)value); return r; });
    }

    public uint renderingLayerMask
    {
        get => ReadSettings().rendering_layer_mask;
        set => Update(r => { r.rendering_layer_mask = value; return r; });
    }

    public int rendererPriority
    {
        get => ReadSettings().renderer_priority;
        set => Update(r => { r.renderer_priority = value; return r; });
    }

    public int sortingLayerID
    {
        get => ReadSettings().sorting_layer;
        set => Update(r => { r.sorting_layer = value; return r; });
    }

    /// <summary>The engine registers no layer but Default (id 0); another name or id throws <see cref="NotSupportedException"/>.</summary>
    public string sortingLayerName
    {
        get => sortingLayerID == 0 ? "Default" : throw new NotSupportedException("named sorting layer is not registered");
        set
        {
            if (value != "Default") throw new NotSupportedException("named sorting layer is not registered");
            sortingLayerID = 0;
        }
    }

    public int sortingOrder
    {
        get => ReadSettings().sorting_order;
        set => Update(r => { r.sorting_order = value; return r; });
    }

    public bool allowOcclusionWhenDynamic
    {
        get => ReadSettings().allow_occlusion_when_dynamic == 1;
        set => Update(r => { r.allow_occlusion_when_dynamic = value ? 1u : 0u; return r; });
    }

    public bool staticShadowCaster
    {
        get => ReadSettings().static_shadow_caster == 1;
        set => Update(r => { r.static_shadow_caster = value ? 1u : 0u; return r; });
    }

    public Matrix4x4 localToWorldMatrix => transform.localToWorldMatrix;

    public Matrix4x4 worldToLocalMatrix => transform.worldToLocalMatrix;

    /// <summary>The mesh whose bounds a renderer without bounds of its own uses, or null.</summary>
    internal virtual Mesh? BoundsMesh => null;

    /// <summary>The box in the object's space. A renderer with a mesh uses the mesh bounds; one without is the
    /// zero box.</summary>
    public virtual Bounds localBounds
    {
        get => BoundsMesh?.bounds ?? default;
        set => throw new NotSupportedException($"{GetType().Name}.localBounds can't be set");
    }

    /// <summary>The world-space box around <see cref="localBounds"/> under <see cref="localToWorldMatrix"/>. It is the
    /// zero box when the renderer has no mesh.</summary>
    public virtual Bounds bounds
    {
        get
        {
            if (BoundsMesh is null) return default;
            var local = localBounds;
            var matrix = localToWorldMatrix;
            var world = new Bounds(matrix.MultiplyPoint3x4(local.center), Vector3.zero);
            foreach (var sx in new[] { -1f, 1f })
                foreach (var sy in new[] { -1f, 1f })
                    foreach (var sz in new[] { -1f, 1f })
                        world.Encapsulate(matrix.MultiplyPoint3x4(local.center + Vector3.Scale(local.extents, new Vector3(sx, sy, sz))));
            return world;
        }
    }

    /// <summary>Copies what the managed component holds from the renderer it was cloned from. <paramref name="remap"/>
    /// maps an object of the original hierarchy to its clone.</summary>
    internal override void CopyManagedState(NativeComponent source, Func<Object, Object> remap)
    {
        var renderer = (Renderer)source;
        _shared = [.. renderer._shared];
        _instances = renderer._instances is null ? null : [.. renderer._instances];
        foreach (var (index, block) in renderer._blocks) _blocks[index] = CopyBlock(block);
    }
}
