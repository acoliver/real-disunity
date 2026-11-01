//! Shaders that script fields and the resource container reference, as plan assets, and the
//! shaders and materials a scene references (see [`crate::player_materials`]).
//!
//! A shader plans as its declared name, render queue and the counts of sub-shaders and of
//! passes in the first sub-shader. Its programs are not planned: the engine builds the shader
//! object by name, and drawing with it is later work. A shader that does not decode, or
//! declares a queue the engine cannot read, plans as an error entry and a reference to it
//! binds `null`.

use std::path::Path;

use worldrunner_unity_serialized::{ClassId, object::PPtr, shader::Shader};

use crate::{
    plan::PlanAsset,
    player_files::PlayerFiles,
    player_materials::{
        decode_material, plan_material, unplanned_reference as unplanned_material_reference,
    },
    player_plan::PlanSource,
    player_resources::ObjectKey,
    player_values::not_held,
};

/// What the plan holds of a shader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShaderDecl {
    pub(crate) name: String,
    pub(crate) render_queue: i32,
    pub(crate) pass_count: usize,
    pub(crate) subshader_count: usize,
}

impl ShaderDecl {
    pub(crate) fn into_asset(self) -> PlanAsset {
        PlanAsset::Shader {
            name: self.name,
            render_queue: self.render_queue,
            pass_count: self.pass_count,
            subshader_count: self.subshader_count,
        }
    }
}

/// The queue of a shader that names none.
const GEOMETRY_QUEUE: i32 = 2000;

/// The queue a name stands for: one of the named queues with an optional offset, such as
/// `Transparent-50`, or a plain number.
fn queue_value(text: &str) -> Result<i32, String> {
    const NAMED: [(&str, i32); 5] = [
        ("background", 1000),
        ("geometry", GEOMETRY_QUEUE),
        ("alphatest", 2450),
        ("transparent", 3000),
        ("overlay", 4000),
    ];
    let text = text.trim();
    if let Ok(number) = text.parse::<i32>() {
        return Ok(number);
    }
    let split = text.find(['+', '-']).unwrap_or(text.len());
    let (name, offset) = text.split_at(split);
    let unknown =
        || format!("the shader declares the queue '{text}', which the engine cannot read");
    let &(_, base) = NAMED
        .iter()
        .find(|(known, _)| known.eq_ignore_ascii_case(name.trim()))
        .ok_or_else(unknown)?;
    let offset = if offset.is_empty() {
        0
    } else {
        offset
            .trim_start_matches('+')
            .parse::<i32>()
            .map_err(|_| unknown())?
    };
    base.checked_add(offset).ok_or_else(unknown)
}

/// The queue the first sub-shader's `Queue` tag names, `Geometry` without one.
fn render_queue(tags: &[(String, String)]) -> Result<i32, String> {
    tags.iter()
        .find(|(key, _)| key.eq_ignore_ascii_case("queue"))
        .map_or(Ok(GEOMETRY_QUEUE), |(_, value)| queue_value(value))
}

/// Decodes the shader object `key` into what the plan holds, or says why it does not.
pub(crate) fn decode_shader(
    files: &mut PlayerFiles,
    key: &ObjectKey,
) -> Result<ShaderDecl, String> {
    let (file, path_id) = key;
    let shader: Shader = files
        .open_path(file)
        .and_then(|open| open.decode(*path_id))
        .map_err(|error| error.to_string())?;
    let name = if shader.parsed.name.is_empty() {
        shader.name
    } else {
        shader.parsed.name
    };
    if name.is_empty() {
        return Err(format!(
            "shader object {path_id} of {} declares no name",
            file.display()
        ));
    }
    let first = shader.parsed.sub_shaders.first();
    Ok(ShaderDecl {
        name,
        render_queue: first.map_or(Ok(GEOMETRY_QUEUE), |sub| render_queue(&sub.tags))?,
        pass_count: first.map_or(0, |sub| sub.passes.len()),
        subshader_count: shader.parsed.sub_shaders.len(),
    })
}

/// Why a reference to a shader that did not plan is left out.
pub(crate) fn unplanned_reference(pptr: PPtr, cause: &str) -> String {
    format!(
        "references shader object {} of file {}, which did not plan: {cause}",
        pptr.path_id, pptr.file_id
    )
}

/// The class of the object a reference names, when the file holds it.
pub(crate) fn class_of(files: &mut PlayerFiles, key: &ObjectKey) -> Option<ClassId> {
    files
        .open_path(&key.0)
        .ok()?
        .file
        .object(key.1)
        .map(|object| object.class_id)
}

/// The source of a scene: the player's files, and the shaders and materials its script fields
/// and renderers reference, which become the scene plan's assets in the order they are first
/// referenced. A material's shader is planned before it.
pub(crate) struct SceneSource {
    files: PlayerFiles,
    pub(crate) assets: Vec<PlanAsset>,
    planned: Vec<(ObjectKey, usize)>,
    /// What the planned materials could not hold.
    pub(crate) differences: Vec<String>,
}

impl SceneSource {
    pub(crate) fn new(files: PlayerFiles) -> Self {
        Self {
            files,
            assets: Vec::new(),
            planned: Vec::new(),
            differences: Vec::new(),
        }
    }

    pub(crate) fn files_mut(&mut self) -> &mut PlayerFiles {
        &mut self.files
    }

    fn plan_asset(
        &mut self,
        key: &ObjectKey,
        class: ClassId,
        pptr: PPtr,
    ) -> Result<PlanAsset, String> {
        if class == ClassId::SHADER {
            let decl = decode_shader(&mut self.files, key)
                .map_err(|cause| unplanned_reference(pptr, &cause))?;
            return Ok(decl.into_asset());
        }
        let material = decode_material(&mut self.files, key)
            .map_err(|cause| unplanned_material_reference(pptr, &cause))?;
        let mut found = Vec::new();
        let planned = plan_material(
            &material,
            |reference| self.asset(&key.0, reference),
            &mut found,
        );
        self.differences.append(&mut found);
        Ok(PlanAsset::Material { material: planned })
    }
}

impl PlanSource for SceneSource {
    fn files(&mut self) -> &mut PlayerFiles {
        &mut self.files
    }

    fn asset(&mut self, owner: &Path, pptr: PPtr) -> Result<usize, String> {
        let file = self
            .files
            .referenced_file(owner, pptr)
            .map_err(|_| not_held(pptr))?;
        let key = (file, pptr.path_id);
        let class = class_of(&mut self.files, &key);
        for held_by_resources in [
            ClassId::TEXTURE_2D,
            ClassId::MESH,
            ClassId::ANIMATOR_CONTROLLER,
            ClassId::ANIMATOR_OVERRIDE_CONTROLLER,
        ] {
            if class == Some(held_by_resources) {
                return Err(format!(
                    "references {held_by_resources} object {} of file {}, which scene plans do not hold",
                    pptr.path_id, pptr.file_id
                ));
            }
        }
        let class = class
            .filter(|&class| class == ClassId::SHADER || class == ClassId::MATERIAL)
            .ok_or_else(|| not_held(pptr))?;
        if let Some((_, index)) = self.planned.iter().find(|(known, _)| *known == key) {
            return Ok(*index);
        }
        let asset = self.plan_asset(&key, class, pptr)?;
        self.assets.push(asset);
        self.planned.push((key, self.assets.len() - 1));
        Ok(self.assets.len() - 1)
    }
}
