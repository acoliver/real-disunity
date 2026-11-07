//! The resources a stage's Unity parameters declare, placed through its binding table and
//! turned into the neutral declarations a program's layout is checked against.

use wgpu::{BindingType, SamplerBindingType, ShaderStages, TextureSampleType};
use worldrunner_unity_serialized::shader_program::{Binding, ProgramParameters, TextureParam};

use super::layout::LayoutError;
use super::module::ModuleBinding;
use crate::binding::{
    BindingAddress, BindingSource, ResourceBinding, ResourceKind, ResourceLayout, StorageAccess,
};
use crate::module::Reflected;

/// What kind of resource a parameter declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    ConstantBuffer,
    Texture {
        dimension: wgpu::TextureViewDimension,
        multisampled: bool,
    },
    Sampler,
    StructuredBuffer,
    UnorderedAccess,
}

/// A declared resource at its prepared set and binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Declared {
    pub(crate) set: u32,
    pub(crate) binding: u32,
    pub(crate) class: Class,
}

fn unsupported(kind: impl Into<String>) -> LayoutError {
    LayoutError::UnsupportedParameter { kind: kind.into() }
}

/// The table entry for a Unity binding, which must be unique across descriptor sets.
pub(crate) fn entry(
    table: &[ModuleBinding],
    unity_binding: i32,
) -> Result<&ModuleBinding, LayoutError> {
    let wanted = u32::try_from(unity_binding).map_err(|_| LayoutError::UnboundResource {
        unity_binding: i64::from(unity_binding),
    })?;
    let mut matches = table.iter().filter(|e| e.unity_binding == wanted);
    match (matches.next(), matches.next()) {
        (Some(found), None) => Ok(found),
        (Some(_), Some(_)) => Err(LayoutError::AmbiguousBinding {
            unity_binding: wanted,
        }),
        _ => Err(LayoutError::UnboundResource {
            unity_binding: i64::from(unity_binding),
        }),
    }
}

/// Unity's `TextureDimension`, with the values of the managed engine's
/// `Disunity/Enums/Rendering.cs`, as a view dimension; `Any`, `None` and unknown have none.
fn view_dimension(code: i8) -> Result<wgpu::TextureViewDimension, LayoutError> {
    match code {
        2 => Ok(wgpu::TextureViewDimension::D2),
        3 => Ok(wgpu::TextureViewDimension::D3),
        4 => Ok(wgpu::TextureViewDimension::Cube),
        5 => Ok(wgpu::TextureViewDimension::D2Array),
        6 => Ok(wgpu::TextureViewDimension::CubeArray),
        other => Err(unsupported(format!("texture dimension {other}"))),
    }
}

/// Every resource `parameters` declares, with duplicates merged.
pub(crate) fn declarations(
    parameters: &ProgramParameters,
    table: &[ModuleBinding],
) -> Result<Vec<Declared>, LayoutError> {
    if !parameters.vectors.is_empty() {
        return Err(unsupported("loose vector"));
    }
    if !parameters.matrices.is_empty() {
        return Err(unsupported("loose matrix"));
    }
    let mut out = Vec::new();
    for binding in &parameters.constant_buffer_bindings {
        out.push(simple(
            table,
            binding,
            "constant buffer",
            Class::ConstantBuffer,
        )?);
    }
    for binding in &parameters.buffers {
        out.push(simple(
            table,
            binding,
            "structured buffer",
            Class::StructuredBuffer,
        )?);
    }
    for uav in &parameters.uavs {
        let placed = entry(table, uav.index)?;
        out.push(declared(placed.set, placed.binding, Class::UnorderedAccess));
    }
    for texture in &parameters.textures {
        out.extend(texture_declarations(table, texture)?);
    }
    for sampler in &parameters.samplers {
        let placed = entry(table, sampler.bind_point)?;
        let binding = placed.sampler_binding.unwrap_or(placed.binding);
        out.push(declared(placed.set, binding, Class::Sampler));
    }
    out.sort_by_key(|d| (d.set, d.binding));
    // A binding declared twice keeps its first declaration, as the old lookup found it.
    out.dedup_by_key(|d| (d.set, d.binding));
    Ok(out)
}

fn declared(set: u32, binding: u32, class: Class) -> Declared {
    Declared {
        set,
        binding,
        class,
    }
}

fn simple(
    table: &[ModuleBinding],
    binding: &Binding,
    what: &str,
    class: Class,
) -> Result<Declared, LayoutError> {
    if binding.array_size > 1 {
        return Err(unsupported(format!("{what} array")));
    }
    let placed = entry(table, binding.index)?;
    Ok(declared(placed.set, placed.binding, class))
}

/// A texture, and the sampler of a combined image sampler that was split.
fn texture_declarations(
    table: &[ModuleBinding],
    texture: &TextureParam,
) -> Result<Vec<Declared>, LayoutError> {
    let placed = entry(table, texture.index)?;
    let class = Class::Texture {
        dimension: view_dimension(texture.dimension)?,
        multisampled: texture.multisampled,
    };
    let mut out = vec![declared(placed.set, placed.binding, class)];
    if let Some(sampler) = placed.sampler_binding {
        out.push(declared(placed.set, sampler, Class::Sampler));
    }
    Ok(out)
}

impl Class {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ConstantBuffer => "a constant buffer",
            Self::Texture { .. } => "a texture",
            Self::Sampler => "a sampler",
            Self::StructuredBuffer => "a structured buffer",
            Self::UnorderedAccess => "a UAV",
        }
    }

    /// Whether the module's own binding is the kind of resource this class declares.
    fn matches(self, ty: &BindingType) -> bool {
        use wgpu::BufferBindingType as Buf;
        match (self, ty) {
            (
                Self::ConstantBuffer,
                BindingType::Buffer {
                    ty: Buf::Uniform, ..
                },
            )
            | (Self::Sampler, BindingType::Sampler(_))
            | (
                Self::UnorderedAccess,
                BindingType::Buffer {
                    ty: Buf::Storage { .. },
                    ..
                },
            )
            | (
                Self::StructuredBuffer,
                BindingType::Buffer {
                    ty: Buf::Storage { read_only: true },
                    ..
                },
            ) => true,
            (
                Self::Texture {
                    dimension,
                    multisampled,
                },
                BindingType::Texture {
                    view_dimension,
                    multisampled: found_multisampled,
                    ..
                },
            ) => dimension == *view_dimension && multisampled == *found_multisampled,
            _ => false,
        }
    }

    /// The kind to declare for a resource the module does not use, which nothing checks.
    fn unchecked_kind(self) -> ResourceKind {
        match self {
            Self::ConstantBuffer => ResourceKind::Uniform { min_size: 0 },
            Self::Texture {
                dimension,
                multisampled,
            } => ResourceKind::Texture {
                dimension,
                sample: TextureSampleType::Float {
                    filterable: !multisampled,
                },
                multisampled,
            },
            Self::Sampler => ResourceKind::Sampler(SamplerBindingType::Filtering),
            Self::StructuredBuffer => ResourceKind::Storage {
                access: StorageAccess::ReadOnly,
                min_size: 0,
            },
            Self::UnorderedAccess => ResourceKind::Storage {
                access: StorageAccess::ReadWrite,
                min_size: 0,
            },
        }
    }
}

/// The neutral declarations of a stage's Unity resources, visible to `visibility`.
///
/// Unity's classes name a kind of resource but not a sample type, access or comparison
/// sampler, so each class that matches what the module declares at its address takes the
/// module's own full kind; a class that does not match is a `Mismatch` in Unity's terms.
pub(crate) fn resource_layout(
    declared: &[Declared],
    reflected: &[Reflected],
    visibility: ShaderStages,
) -> Result<ResourceLayout, LayoutError> {
    let mut bindings = Vec::new();
    for item in declared {
        let found = reflected
            .iter()
            .find(|r| r.set == item.set && r.binding == item.binding);
        let kind = match found {
            None => item.class.unchecked_kind(),
            Some(resource) if item.class.matches(&resource.ty) => {
                ResourceKind::of_binding_type(&resource.ty, 0)
                    .unwrap_or_else(|| item.class.unchecked_kind())
            }
            Some(resource) => {
                return Err(LayoutError::Mismatch {
                    set: item.set,
                    binding: item.binding,
                    declared: item.class.name(),
                    found: format!("{:?}", resource.ty),
                });
            }
        };
        bindings.push(ResourceBinding {
            at: BindingAddress {
                set: item.set,
                binding: item.binding,
            },
            source: BindingSource::Unnamed,
            visibility,
            kind,
        });
    }
    Ok(ResourceLayout::new(bindings))
}
