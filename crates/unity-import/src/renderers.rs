//! Mesh filters, mesh renderers and skinned mesh renderers of a player as plan components.
//!
//! A Mesh or Material asset cannot be planned yet, so a reference to one binds null: the filter
//! has no mesh and a renderer has empty material slots, one per serialized slot. Each such
//! reference is listed once per component.

use serde::Serialize;
use worldrunner_unity_serialized::graphics::{
    MeshFilter, MeshRenderer, Renderer, SkinnedMeshRenderer,
};

use crate::plan::PlanComponent;

/// How many materials one renderer holds in the engine.
const MAX_MATERIALS: usize = 64;

/// The shadow and occlusion settings of a renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PlanShadowing {
    pub dynamic_occludee: bool,
    pub static_shadow_caster: bool,
    /// The script `ShadowCastingMode` value, 0 to 3.
    pub shadow_casting_mode: u8,
}

/// The renderer settings every renderer plan carries. The mode fields hold the script enum
/// values the managed renderer reads; `material_count` is the number of material slots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct PlanRenderer {
    pub enabled: bool,
    pub receive_shadows: bool,
    #[serde(flatten)]
    pub shadowing: PlanShadowing,
    pub motion_vector_mode: u8,
    pub light_probe_usage: u8,
    pub reflection_probe_usage: u8,
    pub rendering_layer_mask: u32,
    pub renderer_priority: i32,
    pub sorting_layer_id: i32,
    pub sorting_order: i32,
    pub material_count: usize,
}

/// The plan filter for a decoded one; a mesh reference is listed in `differences`.
pub(crate) fn plan_mesh_filter(
    owner: &str,
    filter: &MeshFilter,
    differences: &mut Vec<String>,
) -> PlanComponent {
    if !filter.mesh.is_null() {
        differences.push(unplanned(
            owner,
            "MeshFilter",
            "mesh references a Mesh asset",
        ));
    }
    PlanComponent::MeshFilter {}
}

/// The plan mesh renderer for a decoded one.
pub(crate) fn plan_mesh_renderer(
    owner: &str,
    renderer: &MeshRenderer,
    differences: &mut Vec<String>,
) -> PlanComponent {
    PlanComponent::MeshRenderer {
        renderer: plan_renderer(owner, "MeshRenderer", &renderer.renderer, differences),
    }
}

/// The plan skinned mesh renderer for a decoded one.
pub(crate) fn plan_skinned_mesh_renderer(
    owner: &str,
    skinned: &SkinnedMeshRenderer,
    differences: &mut Vec<String>,
) -> PlanComponent {
    if !skinned.mesh.is_null() {
        differences.push(unplanned(
            owner,
            "SkinnedMeshRenderer",
            "mesh references a Mesh asset",
        ));
    }
    PlanComponent::SkinnedMeshRenderer {
        renderer: plan_renderer(owner, "SkinnedMeshRenderer", &skinned.renderer, differences),
        quality: skinned.quality,
        update_when_offscreen: skinned.update_when_offscreen,
    }
}

fn unplanned(owner: &str, class: &str, what: &str) -> String {
    format!("{owner}: {class} {what}, which plans cannot build yet")
}

fn plan_renderer(
    owner: &str,
    class: &str,
    renderer: &Renderer,
    differences: &mut Vec<String>,
) -> PlanRenderer {
    if renderer
        .materials
        .iter()
        .any(|material| !material.is_null())
    {
        differences.push(unplanned(
            owner,
            class,
            "materials reference Material assets",
        ));
    }
    let settings = renderer.settings;
    let mut mode = |name: &str, value: u8, allowed: &[u8]| {
        if allowed.contains(&value) {
            return value;
        }
        differences.push(format!(
            "{owner}: {class} {name} {value} is not known; BlendProbes or the default is used"
        ));
        1
    };
    PlanRenderer {
        enabled: renderer.enabled,
        receive_shadows: settings.receive_shadows != 0,
        shadowing: PlanShadowing {
            dynamic_occludee: settings.dynamic_occludee != 0,
            static_shadow_caster: settings.static_shadow_caster != 0,
            shadow_casting_mode: mode("shadow casting mode", settings.cast_shadows, &[0, 1, 2, 3]),
        },
        motion_vector_mode: mode("motion vector mode", settings.motion_vectors, &[0, 1, 2]),
        light_probe_usage: mode("light probe usage", settings.light_probe_usage, &[0, 1, 2]),
        reflection_probe_usage: mode(
            "reflection probe usage",
            settings.reflection_probe_usage,
            &[0, 1, 2, 3],
        ),
        rendering_layer_mask: renderer.rendering_layer_mask,
        renderer_priority: renderer.renderer_priority,
        sorting_layer_id: renderer.sorting_layer_id,
        sorting_order: renderer.sorting_order.into(),
        material_count: renderer.materials.len().min(MAX_MATERIALS),
    }
}
