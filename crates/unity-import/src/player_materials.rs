//! Materials that script fields, renderers and the resource container reference, as plan assets.
//!
//! A material plans once, whoever references it: its name, the asset index of its planned
//! shader, the queue and keywords it sets, its tags and disabled passes, and every saved
//! property. A texture slot holds the asset index of the planned texture. A shader the plan
//! cannot hold plans as `null`. A texture the engine does not build (a cube or volume it cannot
//! store, a 2D texture it rejects, a reference the plan does not hold) plans as a slot with no
//! asset index and `unsent` set, which is not the game's own null slot, and is listed once per
//! material. A material that does not decode is an error entry, and a reference to it binds `null`.

use std::collections::BTreeSet;

use serde::Serialize;
use worldrunner_unity_serialized::{
    graphics::{Material, TexEnv},
    object::PPtr,
    shader::Shader,
};

use crate::{player_files::PlayerFiles, player_resources::ObjectKey};

/// A texture property of a material: the planned texture, or `None` for an empty slot or a
/// texture the plan does not hold, with the UV scale and offset. `unsent` is set when the slot
/// names a texture the plan does not hold, which the engine sends as named and not sent; a slot
/// that is null in the game's data has no texture and `unsent` clear. A clear `unsent` is left out
/// of the plan, so the plan of a slot the engine sends is unchanged.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PlanTextureSlot {
    pub name: String,
    pub texture: Option<usize>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub unsent: bool,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

/// What the plan holds of a material. `shader` is the asset index of the planned shader, or
/// `None` when the material has none or it did not plan. `render_queue` is the material's own
/// queue, -1 for the shader's. A material saves its color and vector properties in one list; a
/// property its shader declares as a vector is planned in `vectors`, and every other saved
/// property in `colors`, which the engine converts to the project's color space as a game's own
/// colors are (a linear project sends the linear color). `vectors` is left out when empty.
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub vectors: Vec<(String, [f32; 4])>,
    pub textures: Vec<PlanTextureSlot>,
}

/// `ShaderPropertyType` of a vector property in a shader's declaration.
const VECTOR_PROPERTY: i32 = 1;

/// The names of the properties `shader` declares as vectors.
pub(crate) fn declared_vectors(shader: &Shader) -> BTreeSet<String> {
    shader
        .parsed
        .properties
        .iter()
        .filter(|property| property.kind == VECTOR_PROPERTY)
        .map(|property| property.name.clone())
        .collect()
}

impl PlanMaterial {
    /// Moves each saved property named in `declared`, the vectors the material's shader declares,
    /// from `colors` to `vectors`. A material stores a color and a vector in one list, so
    /// without the shader's declaration they cannot be told apart.
    pub(crate) fn separate_vectors(&mut self, declared: &BTreeSet<String>) {
        let saved = std::mem::take(&mut self.colors);
        (self.vectors, self.colors) = saved
            .into_iter()
            .partition(|(name, _)| declared.contains(name));
    }
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

/// The texture slots of one material that name a texture the plan does not hold, and the first
/// reason.
#[derive(Default)]
struct Unbuilt {
    slots: Vec<String>,
    cause: String,
}

impl Unbuilt {
    fn note(&mut self, slot: &str, cause: String) {
        self.slots.push(slot.to_owned());
        if self.cause.is_empty() {
            self.cause = cause;
        }
    }
}

/// The plan slot of texture property `name`: the planned texture, a null slot, or a texture
/// `resolve` cannot give, which is noted in `unbuilt` and planned as unsent.
fn plan_slot(
    name: &str,
    env: &TexEnv,
    resolve: &mut impl FnMut(PPtr) -> Result<usize, String>,
    unbuilt: &mut Unbuilt,
) -> PlanTextureSlot {
    let (texture, unsent) = if env.texture.is_null() {
        (None, false)
    } else {
        match resolve(env.texture) {
            Ok(index) => (Some(index), false),
            Err(cause) => {
                unbuilt.note(name, cause);
                (None, true)
            }
        }
    };
    PlanTextureSlot {
        name: name.to_owned(),
        texture,
        unsent,
        scale: env.scale,
        offset: env.offset,
    }
}

/// The plan material for a decoded one. `resolve` gives the asset index of the shader or
/// texture a reference names, or why there is none; a shader it cannot give binds `null`, and a
/// texture it cannot give plans as unsent. Both are added to `differences`, one line per material
/// for its textures.
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
    let mut unbuilt = Unbuilt::default();
    let textures = material
        .properties
        .textures
        .iter()
        .map(|(slot, env)| plan_slot(slot, env, &mut resolve, &mut unbuilt))
        .collect();
    if !unbuilt.slots.is_empty() {
        differences.push(format!(
            "material {name}: texture slots {} bind unsent: {}",
            unbuilt.slots.join(", "),
            unbuilt.cause
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
        vectors: Vec::new(),
        textures,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pptr(path_id: i64) -> PPtr {
        PPtr {
            file_id: 0,
            path_id,
        }
    }

    fn texture_env(path_id: i64) -> worldrunner_unity_serialized::graphics::TexEnv {
        worldrunner_unity_serialized::graphics::TexEnv {
            texture: pptr(path_id),
            scale: [1.0; 2],
            offset: [0.0; 2],
        }
    }

    fn material_with(slots: &[(&str, i64)]) -> Material {
        Material {
            name: "Sky".to_owned(),
            shader: pptr(1),
            valid_keywords: Vec::new(),
            invalid_keywords: Vec::new(),
            lightmap_flags: 0,
            enable_instancing_variants: false,
            double_sided_gi: false,
            custom_render_queue: -1,
            tags: Vec::new(),
            disabled_shader_passes: Vec::new(),
            properties: worldrunner_unity_serialized::graphics::SavedProperties {
                textures: slots
                    .iter()
                    .map(|(name, path_id)| ((*name).to_owned(), texture_env(*path_id)))
                    .collect(),
                ..Default::default()
            },
            build_texture_stacks: Vec::new(),
        }
    }

    /// Object 1 is the shader, 2 a 2D texture the plan holds as asset 5, 3 a cubemap the plan
    /// does not build; a slot with path id 0 is null in the game's data.
    fn resolve(reference: PPtr) -> Result<usize, String> {
        match reference.path_id {
            1 => Ok(0),
            2 => Ok(5),
            id => Err(format!(
                "references object {id}, which the plan does not hold"
            )),
        }
    }

    #[test]
    fn a_texture_the_plan_does_not_build_is_unsent_and_a_null_slot_is_not() {
        let material = material_with(&[("_Main", 2), ("_Sky", 3), ("_Detail", 0)]);
        let mut differences = Vec::new();
        let planned = plan_material(&material, resolve, &mut differences);
        let slots: Vec<_> = planned
            .textures
            .iter()
            .map(|slot| (slot.name.as_str(), slot.texture, slot.unsent))
            .collect();
        assert_eq!(
            slots,
            [
                ("_Main", Some(5), false),
                ("_Sky", None, true),
                ("_Detail", None, false)
            ]
        );
        assert_eq!(
            differences,
            [
                "material Sky: texture slots _Sky bind unsent: references object 3, which the plan does not hold"
            ]
        );
    }
}
