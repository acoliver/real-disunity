//! Materials and the global shader state.
use super::ordered_globals::ValueKey;
use super::properties::{check_name, upsert};
use super::{
    ComputeBufferId, GraphicsError, MaterialId, PropertyValue, ShaderId, ShaderProperties,
    TextureRef, TextureSlot,
};
use crate::{World, WorldError};

/// Most keywords, or disabled passes, that a material or the global state holds.
const MAX_KEYWORDS: usize = 256;

/// A material: a shader and the values set for it. Every edit raises `generation`.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldMaterial {
    pub shader: ShaderId,
    /// The queue the material renders in; -1 leaves the choice to the shader.
    pub render_queue: i32,
    /// The enabled keywords, sorted.
    pub keywords: Vec<String>,
    /// The passes switched off, by `LightMode`, sorted.
    pub disabled_passes: Vec<String>,
    pub properties: ShaderProperties,
    /// The structured buffers bound to shader properties by name, sorted by name. A buffer's
    /// contents are the compute buffer's, read when a draw runs.
    pub buffers: Vec<(String, ComputeBufferId)>,
    pub generation: u64,
}

/// One change to a material.
#[derive(Clone, Debug, PartialEq)]
pub enum MaterialEdit {
    Shader(ShaderId),
    RenderQueue(i32),
    Keyword {
        name: String,
        enabled: bool,
    },
    /// Switches a pass, named by its `LightMode`, on or off.
    Pass {
        name: String,
        enabled: bool,
    },
    Property(PropertyValue),
    /// Binds a live compute buffer to a property name as a structured buffer, replacing the one
    /// bound to it, or removes the binding when `buffer` is `None`.
    Buffer {
        name: String,
        buffer: Option<ComputeBufferId>,
    },
    /// Removes every property value and buffer binding; keywords and passes stay.
    ClearProperties,
}

/// The shader state every draw sees next to its material: global keywords and properties.
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalShaderState {
    /// The enabled global keywords, sorted.
    pub keywords: Vec<String>,
    pub properties: ShaderProperties,
    /// Raised by every change.
    pub generation: u64,
}

impl GlobalShaderState {
    pub(crate) const fn new() -> Self {
        Self {
            keywords: Vec::new(),
            properties: ShaderProperties {
                floats: Vec::new(),
                ints: Vec::new(),
                vectors: Vec::new(),
                matrices: Vec::new(),
                textures: Vec::new(),
                vector_arrays: Vec::new(),
                matrix_arrays: Vec::new(),
            },
            generation: 0,
        }
    }
}

/// Adds or removes `name` in a sorted list of names.
fn switch_name(list: &mut Vec<String>, name: &str, present: bool) -> Result<(), GraphicsError> {
    check_name(name)?;
    match list.binary_search_by(|held| held.as_str().cmp(name)) {
        Ok(index) if !present => {
            list.remove(index);
        }
        Err(index) if present => {
            if list.len() >= MAX_KEYWORDS {
                return Err(GraphicsError::TooManyKeywords);
            }
            list.insert(index, name.to_owned());
        }
        _ => {}
    }
    Ok(())
}

impl World {
    /// Creates a material of a live shader with the render queue left to the shader, no keywords
    /// and no values.
    pub fn create_material(&mut self, shader: ShaderId) -> Result<MaterialId, WorldError> {
        self.shader(shader)?;
        let material = WorldMaterial {
            shader,
            render_queue: -1,
            keywords: Vec::new(),
            disabled_passes: Vec::new(),
            properties: ShaderProperties::default(),
            buffers: Vec::new(),
            generation: 1,
        };
        Ok(MaterialId(self.graphics.materials.insert(material)?))
    }

    /// Applies one change to a material and raises its generation. A rejected change leaves the
    /// material as it was.
    pub fn edit_material(&mut self, id: MaterialId, edit: MaterialEdit) -> Result<(), WorldError> {
        let edit = self.checked_edit(edit)?;
        self.freeze_material_draws(id)?;
        let material = self
            .graphics
            .materials
            .get_mut(id.0)
            .ok_or(GraphicsError::StaleMaterial)?;
        match edit {
            MaterialEdit::Shader(shader) => material.shader = shader,
            MaterialEdit::RenderQueue(queue) => material.render_queue = queue,
            MaterialEdit::Keyword { name, enabled } => {
                switch_name(&mut material.keywords, &name, enabled)?;
            }
            MaterialEdit::Pass { name, enabled } => {
                switch_name(&mut material.disabled_passes, &name, !enabled)?;
            }
            MaterialEdit::Property(value) => material.properties.set(value)?,
            MaterialEdit::Buffer {
                name,
                buffer: Some(buffer),
            } => upsert(&mut material.buffers, name, buffer)?,
            MaterialEdit::Buffer { name, buffer: None } => {
                material.buffers.retain(|(held, _)| *held != name);
            }
            MaterialEdit::ClearProperties => {
                material.properties = ShaderProperties::default();
                material.buffers.clear();
            }
        }
        material.generation += 1;
        Ok(())
    }

    /// Binds a live compute buffer to a property name of a material as a structured buffer,
    /// replacing the buffer bound to the name, or removes the binding when `buffer` is `None`.
    /// A buffer that is not alive, or a name that is empty or over 256 bytes, is refused and the
    /// material stays as it was.
    pub fn set_material_buffer(
        &mut self,
        id: MaterialId,
        name: String,
        buffer: Option<ComputeBufferId>,
    ) -> Result<(), WorldError> {
        self.edit_material(id, MaterialEdit::Buffer { name, buffer })
    }

    /// Checks what an edit names before the material is touched.
    fn checked_edit(&self, edit: MaterialEdit) -> Result<MaterialEdit, WorldError> {
        match &edit {
            MaterialEdit::Shader(shader) => {
                self.shader(*shader)?;
            }
            MaterialEdit::Property(value) => self.check_property(value)?,
            MaterialEdit::Buffer { name, buffer } => {
                check_name(name)?;
                if let Some(buffer) = buffer {
                    self.compute_buffer(*buffer)?;
                }
            }
            MaterialEdit::RenderQueue(_)
            | MaterialEdit::Keyword { .. }
            | MaterialEdit::Pass { .. }
            | MaterialEdit::ClearProperties => {}
        }
        Ok(edit)
    }

    /// Checks a property value and that a render texture it names is alive, which a caller that
    /// stores the value itself, as a renderer's property block does, needs before it does.
    pub fn check_property(&self, value: &PropertyValue) -> Result<(), WorldError> {
        value.validate()?;
        if let PropertyValue::Texture(_, TextureSlot { texture, .. }) = value {
            self.check_texture(texture)?;
        }
        Ok(())
    }

    /// Checks that a render texture a texture reference names is alive.
    pub(crate) fn check_texture(&self, texture: &TextureRef) -> Result<(), WorldError> {
        match texture {
            TextureRef::Render(id) => self.render_texture(*id).map(drop),
            TextureRef::Asset(0) | TextureRef::Global(_) | TextureRef::Temporary(_) => {
                Err(GraphicsError::InvalidTexture.into())
            }
            TextureRef::Asset(_) | TextureRef::None | TextureRef::Unsent => Ok(()),
        }
    }

    /// Checks a texture that recorded offscreen work reads: what [`World::check_texture`]
    /// accepts, and a temporary render texture under any name id. Whether the temporary is
    /// allocated when the host reaches the record is the host's to check, as it is for a
    /// temporary target.
    pub(crate) fn check_work_texture(&self, texture: &TextureRef) -> Result<(), WorldError> {
        match texture {
            TextureRef::Temporary(_) => Ok(()),
            other => self.check_texture(other),
        }
    }

    /// Removes a material. Renderers and offscreen work that name it keep the stale id. The active
    /// pass is cleared when it was the material's.
    pub fn destroy_material(&mut self, id: MaterialId) -> Result<(), WorldError> {
        self.graphics
            .materials
            .remove(id.0)
            .map(drop)
            .ok_or(WorldError::Graphics(GraphicsError::StaleMaterial))?;
        self.clear_active_pass_of(id);
        Ok(())
    }

    pub fn material(&self, id: MaterialId) -> Result<&WorldMaterial, WorldError> {
        Ok(self
            .graphics
            .materials
            .get(id.0)
            .ok_or(GraphicsError::StaleMaterial)?)
    }

    /// Every live material, in slot order.
    pub fn materials(&self) -> impl Iterator<Item = (MaterialId, &WorldMaterial)> {
        self.graphics
            .materials
            .iter()
            .map(|(bits, material)| (MaterialId(bits), material))
    }

    /// Stores a global shader value, replacing the one of the same kind and name.
    pub fn set_global_property(&mut self, value: PropertyValue) -> Result<(), WorldError> {
        self.check_property(&value)?;
        let key = ValueKey::of(&value);
        self.graphics.keep_global_start();
        self.graphics.global.properties.set(value)?;
        self.graphics.global.generation += 1;
        self.graphics.direct_writes.note_value(key);
        Ok(())
    }

    /// Removes every global property value; the keywords stay.
    pub fn clear_global_properties(&mut self) {
        self.graphics.clear_global_values();
    }

    /// Enables or disables a global keyword.
    pub fn set_global_keyword(&mut self, name: &str, enabled: bool) -> Result<(), WorldError> {
        self.graphics.keep_global_start();
        switch_name(&mut self.graphics.global.keywords, name, enabled)?;
        self.graphics.global.generation += 1;
        self.graphics.direct_writes.note_keyword(name);
        Ok(())
    }

    pub fn global_shader_state(&self) -> &GlobalShaderState {
        &self.graphics.global
    }

    /// What a texture reference names now: a render texture that was destroyed reads as a texture
    /// the engine did not send.
    pub fn live_texture(&self, texture: TextureRef) -> TextureRef {
        match texture {
            TextureRef::Render(id) if self.render_texture(id).is_err() => TextureRef::Unsent,
            other => other,
        }
    }
}
