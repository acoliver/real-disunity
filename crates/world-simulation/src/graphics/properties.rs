//! Named shader values: what a material, a renderer's property block and the global shader
//! state each hold.
use super::{GraphicsError, RenderTextureId};

/// Longest property, keyword or pass name, in bytes.
pub const MAX_NAME_BYTES: usize = 256;
/// Most properties of one kind that one set of properties holds.
pub const MAX_PROPERTIES_PER_KIND: usize = 1024;

/// What a texture property names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextureRef {
    /// A texture the host reads by the identity it knows the texture by.
    Asset(u64),
    /// A render texture of the world.
    Render(RenderTextureId),
    /// No texture.
    None,
}

/// A texture property: the texture and the scale and offset the material applies to it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSlot {
    pub texture: TextureRef,
    pub scale: [f32; 2],
    pub offset: [f32; 2],
}

impl Default for TextureSlot {
    /// No texture, scale one and offset zero.
    fn default() -> Self {
        Self {
            texture: TextureRef::None,
            scale: [1.0; 2],
            offset: [0.0; 2],
        }
    }
}

/// One named value to store in a set of [`ShaderProperties`].
#[derive(Clone, Debug, PartialEq)]
pub enum PropertyValue {
    Float(String, f32),
    Int(String, i32),
    Vector(String, [f32; 4]),
    /// Sixteen floats in column-major order.
    Matrix(String, [f32; 16]),
    Texture(String, TextureSlot),
}

/// Named shader values, one list per kind, each sorted by name. Setting a name that a list holds
/// replaces its value. A name may appear in several kinds.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaderProperties {
    pub floats: Vec<(String, f32)>,
    pub ints: Vec<(String, i32)>,
    pub vectors: Vec<(String, [f32; 4])>,
    pub matrices: Vec<(String, [f32; 16])>,
    pub textures: Vec<(String, TextureSlot)>,
}

/// Replaces the value of `name` in a list sorted by name, or inserts it in place.
fn upsert<T>(list: &mut Vec<(String, T)>, name: String, value: T) -> Result<(), GraphicsError> {
    match list.binary_search_by(|(held, _)| held.as_str().cmp(&name)) {
        Ok(index) => list[index].1 = value,
        Err(index) => {
            if list.len() >= MAX_PROPERTIES_PER_KIND {
                return Err(GraphicsError::TooManyProperties);
            }
            list.insert(index, (name, value));
        }
    }
    Ok(())
}

fn find<T: Copy>(list: &[(String, T)], name: &str) -> Option<T> {
    let index = list
        .binary_search_by(|(held, _)| held.as_str().cmp(name))
        .ok()?;
    Some(list[index].1)
}

/// Checks a property, keyword or pass name.
pub(super) fn check_name(name: &str) -> Result<(), GraphicsError> {
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return Err(GraphicsError::InvalidName);
    }
    Ok(())
}

fn finite(values: &[f32]) -> Result<(), GraphicsError> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(GraphicsError::NotFinite)
    }
}

impl PropertyValue {
    /// The name the value is stored under.
    pub fn name(&self) -> &str {
        match self {
            Self::Float(name, _)
            | Self::Int(name, _)
            | Self::Vector(name, _)
            | Self::Matrix(name, _)
            | Self::Texture(name, _) => name,
        }
    }

    /// Checks the name and that every float is finite. Whether a render texture is alive is the
    /// world's to check.
    pub fn validate(&self) -> Result<(), GraphicsError> {
        check_name(self.name())?;
        match self {
            Self::Float(_, value) => finite(&[*value]),
            Self::Int(..) => Ok(()),
            Self::Vector(_, value) => finite(value),
            Self::Matrix(_, value) => finite(value),
            Self::Texture(_, slot) => {
                if slot.texture == TextureRef::Asset(0) {
                    return Err(GraphicsError::InvalidTexture);
                }
                finite(&slot.scale)?;
                finite(&slot.offset)
            }
        }
    }
}

impl ShaderProperties {
    /// Stores a value, replacing the one of the same kind and name.
    pub fn set(&mut self, value: PropertyValue) -> Result<(), GraphicsError> {
        value.validate()?;
        match value {
            PropertyValue::Float(name, value) => upsert(&mut self.floats, name, value),
            PropertyValue::Int(name, value) => upsert(&mut self.ints, name, value),
            PropertyValue::Vector(name, value) => upsert(&mut self.vectors, name, value),
            PropertyValue::Matrix(name, value) => upsert(&mut self.matrices, name, value),
            PropertyValue::Texture(name, value) => upsert(&mut self.textures, name, value),
        }
    }

    pub fn float(&self, name: &str) -> Option<f32> {
        find(&self.floats, name)
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        find(&self.ints, name)
    }

    pub fn vector(&self, name: &str) -> Option<[f32; 4]> {
        find(&self.vectors, name)
    }

    pub fn matrix(&self, name: &str) -> Option<[f32; 16]> {
        find(&self.matrices, name)
    }

    pub fn texture(&self, name: &str) -> Option<TextureSlot> {
        find(&self.textures, name)
    }

    /// Whether no value of any kind is stored.
    pub fn is_empty(&self) -> bool {
        self.floats.is_empty()
            && self.ints.is_empty()
            && self.vectors.is_empty()
            && self.matrices.is_empty()
            && self.textures.is_empty()
    }
}
