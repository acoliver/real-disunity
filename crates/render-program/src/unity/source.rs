//! The two forms a stage's parameters take, reduced to one list of wanted resources.
//!
//! Layouts and targets read only the lists here, so a form adds an implementation of
//! [`Source`] and nothing else.

use worldrunner_unity_serialized::shader_parameters::{
    DescriptorSlot, ResourceKind, VariantParameters,
};
use worldrunner_unity_serialized::shader_program::{Binding, ProgramParameters, TextureParam};

use super::buffers::{BufferLayout, ParamName};
use super::declare::Class;
use super::layout::LayoutError;
use super::module::StageParameters;

/// Where Unity's module binds a resource: a binding, and the descriptor set when the
/// parameters name one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct At {
    pub(crate) set: Option<u32>,
    pub(crate) binding: i32,
}

impl At {
    fn binding(binding: i32) -> Self {
        Self { set: None, binding }
    }

    fn slot(slot: DescriptorSlot) -> Self {
        Self {
            set: Some(u32::from(slot.set)),
            binding: i32::from(slot.binding),
        }
    }
}

/// A resource the parameters declare. `stages` are the stage bits that use it, or `None` when
/// the parameters do not say.
pub(crate) struct Wanted {
    pub(crate) at: At,
    pub(crate) class: Result<Class, LayoutError>,
    pub(crate) stages: Option<u8>,
}

/// A constant buffer binding with the layout of its members.
pub(crate) struct WantedBuffer {
    pub(crate) at: At,
    pub(crate) layout: Result<BufferLayout, LayoutError>,
    pub(crate) stages: Option<u8>,
}

/// A texture binding with the name of the property that fills it.
pub(crate) struct WantedTexture {
    pub(crate) at: At,
    pub(crate) name: ParamName,
    pub(crate) stages: Option<u8>,
}

/// A storage buffer binding with the name the host binds a buffer under.
pub(crate) struct WantedStorage {
    pub(crate) at: At,
    pub(crate) name: ParamName,
    pub(crate) stages: Option<u8>,
}

pub(crate) trait Source {
    /// Every resource declared, whichever stage uses it.
    ///
    /// # Errors
    /// Fails on a kind of parameter this renderer does not bind at all.
    fn resources(&self) -> Result<Vec<Wanted>, LayoutError>;

    fn constant_buffers(&self) -> Vec<WantedBuffer>;

    fn textures(&self) -> Vec<WantedTexture>;

    /// The structured buffers and unordered access views, which hold the buffers a host binds.
    fn storage_buffers(&self) -> Vec<WantedStorage>;
}

impl Source for StageParameters {
    fn resources(&self) -> Result<Vec<Wanted>, LayoutError> {
        match self {
            Self::Program(parameters) => parameters.resources(),
            Self::Variant(parameters) => parameters.resources(),
        }
    }

    fn constant_buffers(&self) -> Vec<WantedBuffer> {
        match self {
            Self::Program(parameters) => parameters.constant_buffers(),
            Self::Variant(parameters) => parameters.constant_buffers(),
        }
    }

    fn textures(&self) -> Vec<WantedTexture> {
        match self {
            Self::Program(parameters) => parameters.textures(),
            Self::Variant(parameters) => parameters.textures(),
        }
    }

    fn storage_buffers(&self) -> Vec<WantedStorage> {
        match self {
            Self::Program(parameters) => parameters.storage_buffers(),
            Self::Variant(parameters) => parameters.storage_buffers(),
        }
    }
}

fn unsupported(kind: impl Into<String>) -> LayoutError {
    LayoutError::UnsupportedParameter { kind: kind.into() }
}

fn wanted(at: At, class: Result<Class, LayoutError>, stages: Option<u8>) -> Wanted {
    Wanted { at, class, stages }
}

impl Source for ProgramParameters {
    fn resources(&self) -> Result<Vec<Wanted>, LayoutError> {
        if !self.vectors.is_empty() {
            return Err(unsupported("loose vector"));
        }
        if !self.matrices.is_empty() {
            return Err(unsupported("loose matrix"));
        }
        let mut out = Vec::new();
        for binding in &self.constant_buffer_bindings {
            let class = simple(binding, "constant buffer", Class::ConstantBuffer);
            out.push(wanted(At::binding(binding.index), class, None));
        }
        for binding in &self.buffers {
            let class = simple(binding, "structured buffer", Class::StructuredBuffer);
            out.push(wanted(At::binding(binding.index), class, None));
        }
        for uav in &self.uavs {
            let at = At::binding(uav.index);
            out.push(wanted(at, Ok(Class::UnorderedAccess), None));
        }
        for texture in &self.textures {
            let at = At::binding(texture.index);
            out.push(wanted(at, program_texture_class(texture), None));
        }
        for sampler in &self.samplers {
            let at = At::binding(sampler.bind_point);
            out.push(wanted(at, Ok(Class::Sampler), None));
        }
        Ok(out)
    }

    fn constant_buffers(&self) -> Vec<WantedBuffer> {
        self.constant_buffer_bindings
            .iter()
            .map(|binding| {
                let layout = self
                    .constant_buffers
                    .iter()
                    .find(|buffer| buffer.name_index == binding.name_index)
                    .map(BufferLayout::from_program)
                    .ok_or(LayoutError::UndescribedConstantBuffer {
                        name_index: binding.name_index,
                    });
                WantedBuffer {
                    at: At::binding(binding.index),
                    layout,
                    stages: None,
                }
            })
            .collect()
    }

    fn textures(&self) -> Vec<WantedTexture> {
        self.textures
            .iter()
            .map(|texture| WantedTexture {
                at: At::binding(texture.index),
                name: ParamName::Index(texture.name_index),
                stages: None,
            })
            .collect()
    }

    fn storage_buffers(&self) -> Vec<WantedStorage> {
        let structured = self.buffers.iter().map(|buffer| WantedStorage {
            at: At::binding(buffer.index),
            name: ParamName::Index(buffer.name_index),
            stages: None,
        });
        let writable = self.uavs.iter().map(|uav| WantedStorage {
            at: At::binding(uav.index),
            name: ParamName::Index(uav.name_index),
            stages: None,
        });
        structured.chain(writable).collect()
    }
}

fn simple(binding: &Binding, what: &str, class: Class) -> Result<Class, LayoutError> {
    if binding.array_size > 1 {
        return Err(unsupported(format!("{what} array")));
    }
    Ok(class)
}

/// Unity's `TextureDimension`, with the values of the managed engine's
/// `Disunity/Enums/Rendering.cs`, as a view dimension; `Any`, `None` and unknown have none.
fn program_dimension(code: i8) -> Result<wgpu::TextureViewDimension, LayoutError> {
    match code {
        2 => Ok(wgpu::TextureViewDimension::D2),
        3 => Ok(wgpu::TextureViewDimension::D3),
        4 => Ok(wgpu::TextureViewDimension::Cube),
        5 => Ok(wgpu::TextureViewDimension::D2Array),
        6 => Ok(wgpu::TextureViewDimension::CubeArray),
        other => Err(unsupported(format!("texture dimension {other}"))),
    }
}

fn program_texture_class(texture: &TextureParam) -> Result<Class, LayoutError> {
    Ok(Class::Texture {
        dimension: program_dimension(texture.dimension)?,
        multisampled: texture.multisampled,
    })
}

/// A player texture's dimension code, as measured against the SPIR-V of the Old World player:
/// 4, 6, 8 and 10 are 2D, 3D, cube and 2D array textures. Codes 5 and 11 were measured last
/// (6 and 3 bindings) as multisampled 2D and multisampled 2D array images, which this
/// renderer has no way to bind, so they are named errors.
fn variant_dimension(code: i32) -> Result<wgpu::TextureViewDimension, LayoutError> {
    match code {
        4 => Ok(wgpu::TextureViewDimension::D2),
        6 => Ok(wgpu::TextureViewDimension::D3),
        8 => Ok(wgpu::TextureViewDimension::Cube),
        10 => Ok(wgpu::TextureViewDimension::D2Array),
        5 | 11 => Err(LayoutError::MultisampledTexture { code }),
        other => Err(unsupported(format!("texture dimension {other}"))),
    }
}

fn variant_class(kind: &ResourceKind) -> Result<Class, LayoutError> {
    match kind {
        ResourceKind::Texture { dimension, .. } => Ok(Class::Texture {
            dimension: variant_dimension(*dimension)?,
            multisampled: false,
        }),
        ResourceKind::ConstantBuffer => Ok(Class::ConstantBuffer),
        ResourceKind::Buffer => Ok(Class::StorageBuffer),
        ResourceKind::StorageImage => Err(unsupported("storage image")),
        ResourceKind::Sampler { .. } => Ok(Class::Sampler),
    }
}

impl Source for VariantParameters {
    fn resources(&self) -> Result<Vec<Wanted>, LayoutError> {
        let mut out = Vec::new();
        for binding in &self.bindings {
            let slot = binding.slot;
            out.push(wanted(
                At::slot(slot),
                variant_class(&binding.kind),
                Some(slot.stages),
            ));
            if let ResourceKind::Texture {
                sampler: Some(sampler),
                ..
            } = binding.kind
            {
                out.push(wanted(
                    At::slot(sampler),
                    Ok(Class::Sampler),
                    Some(sampler.stages),
                ));
            }
        }
        Ok(out)
    }

    fn constant_buffers(&self) -> Vec<WantedBuffer> {
        self.bindings
            .iter()
            .filter(|binding| binding.kind == ResourceKind::ConstantBuffer)
            .map(|binding| {
                let layout = self
                    .constant_buffers
                    .iter()
                    .find(|buffer| buffer.name == binding.name)
                    .map(BufferLayout::from_variant)
                    .ok_or_else(|| LayoutError::UndescribedBuffer {
                        name: binding.name.clone(),
                    });
                WantedBuffer {
                    at: At::slot(binding.slot),
                    layout,
                    stages: Some(binding.slot.stages),
                }
            })
            .collect()
    }

    fn textures(&self) -> Vec<WantedTexture> {
        self.bindings
            .iter()
            .filter(|binding| matches!(binding.kind, ResourceKind::Texture { .. }))
            .map(|binding| WantedTexture {
                at: At::slot(binding.slot),
                name: ParamName::Named(binding.name.clone()),
                stages: Some(binding.slot.stages),
            })
            .collect()
    }

    fn storage_buffers(&self) -> Vec<WantedStorage> {
        self.bindings
            .iter()
            .filter(|binding| binding.kind == ResourceKind::Buffer)
            .map(|binding| WantedStorage {
                at: At::slot(binding.slot),
                name: ParamName::Named(binding.name.clone()),
                stages: Some(binding.slot.stages),
            })
            .collect()
    }
}
