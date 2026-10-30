using System.Text.Json;
using UnityEngine;
using UnityEngine.Rendering;

namespace Disunity.Hosting;

internal sealed partial class SceneLoader
{
    /// <summary>A mesh filter of a player plan. The plan holds no mesh asset yet, so the filter has no mesh.</summary>
    private static MeshFilter MeshFilter(GameObject owner) => owner.AddComponent<MeshFilter>();

    private static MeshRenderer MeshRenderer(GameObject owner, JsonElement value)
    {
        var renderer = owner.AddComponent<MeshRenderer>();
        ApplyRenderer(renderer, value);
        return renderer;
    }

    private static SkinnedMeshRenderer SkinnedMeshRenderer(GameObject owner, JsonElement value)
    {
        var renderer = owner.AddComponent<SkinnedMeshRenderer>();
        ApplyRenderer(renderer, value);
        renderer.quality = (SkinQuality)value.Int("quality");
        renderer.updateWhenOffscreen = value.Bool("update_when_offscreen");
        return renderer;
    }

    /// <summary>Sets the renderer settings of a plan. The plan holds no material asset yet, so every material slot is empty.</summary>
    private static void ApplyRenderer(Renderer renderer, JsonElement value)
    {
        renderer.enabled = value.Bool("enabled");
        renderer.receiveShadows = value.Bool("receive_shadows");
        renderer.allowOcclusionWhenDynamic = value.Bool("dynamic_occludee");
        renderer.staticShadowCaster = value.Bool("static_shadow_caster");
        renderer.shadowCastingMode = (ShadowCastingMode)value.Int("shadow_casting_mode");
        renderer.motionVectorGenerationMode = (MotionVectorGenerationMode)value.Int("motion_vector_mode");
        renderer.lightProbeUsage = (LightProbeUsage)value.Int("light_probe_usage");
        renderer.reflectionProbeUsage = (ReflectionProbeUsage)value.Int("reflection_probe_usage");
        renderer.renderingLayerMask = unchecked((uint)value.Mask("rendering_layer_mask"));
        renderer.rendererPriority = value.Int("renderer_priority");
        renderer.sortingLayerID = value.Int("sorting_layer_id");
        renderer.sortingOrder = value.Int("sorting_order");
        renderer.sharedMaterials = new Material?[value.Int("material_count")];
    }
}
