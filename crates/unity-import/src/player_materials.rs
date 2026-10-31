//! Materials that script fields, renderers and the resource container reference, as plan assets.
//!
//! A material plans once, whoever references it: its name, the asset index of its planned
//! shader, the queue and keywords it sets, its tags and disabled passes, and every saved
//! property. A texture slot holds the asset index of the planned texture. A reference the plan
//! cannot hold (a shader that did not plan, a texture the engine does not build, a scene plan,
//! which holds no textures) plans as `null` and is listed once per material. A material that
//! does not decode is an error entry, and a reference to it binds `null`.

use serde::Serialize;
use worldrunner_unity_serialized::{graphics::Material, object::PPtr};

use crate::{player_files::PlayerFiles, player_resources::ObjectKey};

/// A texture property of a material: the planned texture, or `None` for an empty slot or a
/// texture the plan does not hold, with the UV scale and offset.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanTextureSlot {
    pub name: String,
    pub texture: Option<usize>,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

/// What the plan holds of a material. `shader` is the asset index of the planned shader, or
/// `None` when the material has none or it did not plan. `render_queue` is the material's own
/// queue, -1 for the shader's. Colors include the vector properties, which a material stores
/// in the same list.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanMaterial {
    pub name: String,
    pub shader: Option<usize>,
    pub render_queue: i32,
    pub keywords: Vec<String>,
    pub tags: Vec<(String, String)>,
    pub disabled_passes: Vec<String>,
    pub floats: Vec<(String, f32)>,
    pub ints: Vec<(String, i32)>,
    pub colors: Vec<(String, [f32; 4])>,
    pub textures: Vec<PlanTextureSlot>,
}

/// Decodes the material object `key`, or says why it does not.
pub(crate) fn decode_material(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<Material, String> {
    let (file, path_id) = key;
    let material: Material = files
        .open_path(file)
        .and_then(|open| open.decode(*path_id))
        .map_err(|error| error.to_string())?;
    Ok(material)
}

/// Why a reference to a material that did not plan is left out.
pub(crate) fn unplanned_reference(pptr: PPtr, cause: &str) -> String {
    format!(
        "references material object {} of file {}, which did not plan: {cause}",
        pptr.path_id, pptr.file_id
    )
}

/// The plan material for a decoded one. `resolve` gives the asset index of the shader or
/// texture a reference names, or why there is none; what it cannot give binds `null` and is
/// added to `differences`, one line per material for its textures.
pub(crate) fn plan_material(
    material: &Material,
    mut resolve: impl FnMut(PPtr) -> Result<usize, String>,
    differences: &mut Vec<String>,
) -> PlanMaterial {
    let name = &material.name;
    let shader = if material.shader.is_null() {
        differences.push(format!(
            "material {name}: references no shader; the engine's fallback shader is used"
        ));
        None
    } else {
        resolve(material.shader)
            .map_err(|cause| differences.push(format!("material {name}: shader {cause}")))
            .ok()
    };
    let mut missing = Vec::new();
    let mut cause = String::new();
    let textures = material
        .properties
        .textures
        .iter()
        .map(|(slot, env)| {
            let texture = if env.texture.is_null() {
                None
            } else {
                resolve(env.texture)
                    .map_err(|why| {
                        missing.push(slot.as_str());
                        if cause.is_empty() {
                            cause = why;
                        }
                    })
                    .ok()
            };
            PlanTextureSlot {
                name: slot.clone(),
                texture,
                scale: env.scale,
                offset: env.offset,
            }
        })
        .collect();
    if !missing.is_empty() {
        differences.push(format!(
            "material {name}: texture slots {} bind null: {cause}",
            missing.join(", ")
        ));
    }
    PlanMaterial {
        name: name.clone(),
        shader,
        render_queue: material.custom_render_queue,
        keywords: material.valid_keywords.clone(),
        tags: material.tags.clone(),
        disabled_passes: material.disabled_shader_passes.clone(),
        floats: material.properties.floats.clone(),
        ints: material.properties.ints.clone(),
        colors: material.properties.colors.clone(),
        textures,
    }
}
