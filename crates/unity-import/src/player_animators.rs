//! `Animator` components of a player and the controller assets they reference, as plan
//! components and assets.
//!
//! An animator plans with its settings and the asset index of its controller, which is an
//! `AnimatorController` or an `AnimatorOverrideController`. A controller plans with the clips
//! it lists and an override controller with its base controller and the clip pairs it replaces;
//! a clip plans as its name only, since the engine plays no animation yet. Each distinct
//! controller and clip plans once, whoever references it. An avatar is not planned: an animator
//! that names one lists it. A controller or clip reference the plan cannot hold binds null and
//! is listed.

use serde::Serialize;
use worldrunner_unity_serialized::{
    ClassId,
    animator::{AnimationClip, Animator, AnimatorOverrideController},
    animator_controller::AnimatorController,
    object::PPtr,
};

use crate::{
    PlayerError,
    plan::PlanComponent,
    player_resources::{ObjectKey, Registry, ResourceAsset},
};

/// A clip of an override controller's base and the clip that replaces it; a null replacement
/// removes the original.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PlanClipOverride {
    pub original: Option<usize>,
    pub replacement: Option<usize>,
}

/// The plan animator for a decoded one. `resolve` gives the asset index of its controller; a
/// reference it cannot resolve is `None` and listed in `differences`.
pub(crate) fn plan_animator(
    owner: &str,
    animator: &Animator,
    resolve: impl FnOnce(PPtr) -> Result<usize, String>,
    differences: &mut Vec<String>,
) -> PlanComponent {
    if !animator.avatar.is_null() {
        differences.push(format!(
            "{owner}: Animator avatar references an Avatar asset, which plans cannot build yet"
        ));
    }
    let controller = if animator.controller.is_null() {
        None
    } else {
        resolve(animator.controller)
            .map_err(|cause| differences.push(format!("{owner}: Animator controller {cause}")))
            .ok()
    };
    PlanComponent::Animator {
        enabled: animator.enabled,
        controller,
        culling_mode: animator.culling_mode,
        update_mode: animator.update_mode,
        apply_root_motion: animator.motion.apply_root_motion,
        linear_velocity_blending: animator.motion.linear_velocity_blending,
        stabilize_feet: animator.motion.stabilize_feet,
        keep_state_on_disable: animator.on_disable.keep_state,
        write_defaults_on_disable: animator.on_disable.write_defaults,
    }
}

/// The asset index of the object `pptr` names, or `None` for a null reference and for one the
/// plan cannot hold, which is noted as `place` references it.
fn referenced(registry: &mut Registry, key: &ObjectKey, pptr: PPtr, place: &str) -> Option<usize> {
    if pptr.is_null() {
        return None;
    }
    registry
        .asset_index(&key.0, pptr)
        .map_err(|cause| registry.notes.push(format!("{place} {cause}")))
        .ok()
}

/// The plan asset for the animation object `key` of `class`, which is a clip, a controller or
/// an override controller.
///
/// # Errors
/// Fails when the object does not decode.
pub(crate) fn plan_animation_asset(
    registry: &mut Registry,
    key: &ObjectKey,
    class: ClassId,
) -> Result<ResourceAsset, PlayerError> {
    let (file, path_id) = key;
    if class == ClassId::ANIMATION_CLIP {
        let clip: AnimationClip = registry.files.open_path(file)?.decode(*path_id)?;
        return Ok(ResourceAsset::AnimationClip { name: clip.name });
    }
    if class == ClassId::ANIMATOR_CONTROLLER {
        let controller: AnimatorController = registry.files.open_path(file)?.decode(*path_id)?;
        let place = format!("animator controller {}: clip", controller.name);
        let clips = (controller.clips.iter())
            .map(|&clip| referenced(registry, key, clip, &place))
            .collect();
        return Ok(ResourceAsset::AnimatorController {
            name: controller.name,
            clips,
        });
    }
    let controller: AnimatorOverrideController =
        registry.files.open_path(file)?.decode(*path_id)?;
    let place = format!("animator override controller {}:", controller.name);
    let base_controller = referenced(
        registry,
        key,
        controller.base_controller,
        &format!("{place} base controller"),
    );
    let overrides = (controller.overrides.iter())
        .map(|pair| PlanClipOverride {
            original: referenced(
                registry,
                key,
                pair.original,
                &format!("{place} original clip"),
            ),
            replacement: referenced(
                registry,
                key,
                pair.replacement,
                &format!("{place} replacement clip"),
            ),
        })
        .collect();
    Ok(ResourceAsset::AnimatorOverrideController {
        name: controller.name,
        base_controller,
        overrides,
    })
}
