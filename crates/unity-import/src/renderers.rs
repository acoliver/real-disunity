//! Mesh filters, mesh renderers and skinned mesh renderers of a player as plan components.
//!
//! A filter plans the asset index of its mesh (see [`crate::player_meshes`]); a mesh the plan
//! cannot hold binds null and is listed once per component. A renderer plans one material slot per serialized slot (see
//! [`crate::player_materials`]): the asset index of the planned material, or null for an empty
//! slot or a material the plan cannot hold, which is listed.

use serde::Serialize;
use worldrunner_unity_serialized::{
    graphics::{MeshFilter, MeshRenderer, Renderer, SkinnedMeshRenderer},
    object::PPtr,
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
/// values the managed renderer reads; `materials` holds one entry per material slot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
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
    pub materials: Vec<Option<usize>>,
}

/// The plan filter for a decoded one. `resolve` gives the asset index of its mesh; a reference
/// it cannot resolve is `None` and listed in `differences`.
pub(crate) fn plan_mesh_filter(
    owner: &str,
    filter: &MeshFilter,
    resolve: impl FnOnce(PPtr) -> Result<usize, String>,
    differences: &mut Vec<String>,
) -> PlanComponent {
    let mesh = if filter.mesh.is_null() {
        None
    } else {
        resolve(filter.mesh)
            .map_err(|cause| differences.push(format!("{owner}: MeshFilter mesh {cause}")))
            .ok()
    };
    PlanComponent::MeshFilter { mesh }
}

/// The plan mesh renderer for a decoded one.
pub(crate) fn plan_mesh_renderer(
    owner: &str,
    renderer: &MeshRenderer,
    materials: Vec<Option<usize>>,
    differences: &mut Vec<String>,
) -> PlanComponent {
    PlanComponent::MeshRenderer {
        renderer: plan_renderer(
            owner,
            "MeshRenderer",
            &renderer.renderer,
            materials,
            differences,
        ),
    }
}

/// The plan skinned mesh renderer for a decoded one.
pub(crate) fn plan_skinned_mesh_renderer(
    owner: &str,
    skinned: &SkinnedMeshRenderer,
    materials: Vec<Option<usize>>,
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
        renderer: plan_renderer(
            owner,
            "SkinnedMeshRenderer",
            &skinned.renderer,
            materials,
            differences,
        ),
        quality: skinned.quality,
        update_when_offscreen: skinned.update_when_offscreen,
    }
}

fn unplanned(owner: &str, class: &str, what: &str) -> String {
    format!("{owner}: {class} {what}, which plans cannot build yet")
}

/// The asset index `resolve` gives for each material slot, in the first [`MAX_MATERIALS`]
/// slots. A reference it cannot resolve is `None` and listed in `differences`; an empty slot
/// is `None` and not.
pub(crate) fn plan_material_slots(
    owner: &str,
    class: &str,
    materials: &[PPtr],
    mut resolve: impl FnMut(PPtr) -> Result<usize, String>,
    differences: &mut Vec<String>,
) -> Vec<Option<usize>> {
    materials
        .iter()
        .take(MAX_MATERIALS)
        .enumerate()
        .map(|(slot, &material)| {
            if material.is_null() {
                return None;
            }
            resolve(material)
                .map_err(|cause| {
                    differences.push(format!("{owner}: {class} material {slot} {cause}"));
                })
                .ok()
        })
        .collect()
}

fn plan_renderer(
    owner: &str,
    class: &str,
    renderer: &Renderer,
    materials: Vec<Option<usize>>,
    differences: &mut Vec<String>,
) -> PlanRenderer {
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
        materials,
    }
}
