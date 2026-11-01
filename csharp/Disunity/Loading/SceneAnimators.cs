using System.Text.Json;
using UnityEngine;

namespace Disunity.Hosting;

internal sealed partial class SceneLoader
{
    /// <summary>An animator of a player plan: its settings and <c>"controller"</c>, the asset index of its planned controller or
    /// null. Nothing plays the controller's clips yet.</summary>
    private Animator Animator(GameObject owner, JsonElement value)
    {
        var animator = owner.AddComponent<Animator>();
        if (value.Index("controller") is { } controller) animator.runtimeAnimatorController = (RuntimeAnimatorController)PlannedAsset(controller);
        animator.cullingMode = (AnimatorCullingMode)value.Int("culling_mode");
        animator.updateMode = (AnimatorUpdateMode)value.Int("update_mode");
        animator.applyRootMotion = value.Bool("apply_root_motion");
        animator.linearVelocityBlending = value.Bool("linear_velocity_blending");
        animator.stabilizeFeet = value.Bool("stabilize_feet");
        animator.keepAnimatorStateOnDisable = value.Bool("keep_state_on_disable");
        animator.writeDefaultValuesOnDisable = value.Bool("write_defaults_on_disable");
        animator.enabled = value.Bool("enabled");
        return animator;
    }
}
