//! The resources a stage's Unity parameters declare, placed through its binding table and
//! turned into the neutral declarations a program's layout is checked against.

use wgpu::{BindingType, SamplerBindingType, ShaderStages, TextureSampleType};
use worldrunner_unity_serialized::shader_parameters::DescriptorSlot;

use super::layout::LayoutError;
use super::module::{Reflected, ResourceBinding, StageKind, StageParameters, stage_bit};
use super::source::{At, Source};
use crate::binding::{self, BindingAddress, BindingSource, ResourceKind, StorageAccess};

/// What kind of resource a parameter declares.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Class {
    ConstantBuffer,
    Texture {
        dimension: wgpu::TextureViewDimension,
        multisampled: bool,
    },
    Sampler,
    /// A read-only storage buffer.
    StructuredBuffer,
    /// A storage buffer the shader may write.
    UnorderedAccess,
    /// A storage buffer whose access the module decides.
    StorageBuffer,
    /// A storage image, whose access, format and dimension the module decides.
    StorageImage,
}

/// A declared resource at its prepared set and binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Declared {
    pub(crate) set: u32,
    pub(crate) binding: u32,
    pub(crate) class: Class,
    /// The stage bits that use the resource, or `None` when the parameters do not say.
    pub(crate) stages: Option<u8>,
}

impl Declared {
    /// Whether `kind`'s stage uses the resource. Parameters that name no stages, and stages
    /// with no bit, leave every resource to the module.
    pub(crate) fn applies(&self, kind: StageKind) -> bool {
        applies(self.stages, kind)
    }

    /// The stages the resource is visible to, from the stage bits.
    fn visibility(&self) -> ShaderStages {
        let mask = self.stages.unwrap_or_default();
        let mut visibility = ShaderStages::empty();
        if mask & DescriptorSlot::VERTEX != 0 {
            visibility |= ShaderStages::VERTEX;
        }
        if mask & DescriptorSlot::FRAGMENT != 0 {
            visibility |= ShaderStages::FRAGMENT;
        }
        visibility
    }
}

pub(crate) fn applies(stages: Option<u8>, kind: StageKind) -> bool {
    match (stages, stage_bit(kind)) {
        (Some(mask), Some(bit)) => mask & bit != 0,
        _ => true,
    }
}

/// The table entry for a Unity binding. Without a set, the binding must be unique across
/// descriptor sets.
pub(crate) fn entry(table: &[ResourceBinding], at: At) -> Result<&ResourceBinding, LayoutError> {
    let unbound = || match at.set {
        Some(set) => LayoutError::UnboundSlot {
            set,
            binding: u32::try_from(at.binding).unwrap_or_default(),
        },
        None => LayoutError::UnboundResource {
            unity_binding: i64::from(at.binding),
        },
    };
    let wanted = u32::try_from(at.binding).map_err(|_| unbound())?;
    let mut matches = table
        .iter()
        .filter(|e| e.unity_binding == wanted && at.set.is_none_or(|set| e.set == set));
    match (matches.next(), matches.next()) {
        (Some(found), None) => Ok(found),
        (Some(_), Some(_)) => Err(LayoutError::AmbiguousBinding {
            unity_binding: wanted,
        }),
        _ => Err(unbound()),
    }
}

/// The resources a resource of `class` takes in the prepared module: a texture and the sampler
/// split off its combined image sampler, or a sampler, which is the split one when there is
/// one.
fn placed(
    entry: &ResourceBinding,
    class: Class,
    stages: Option<u8>,
) -> impl Iterator<Item = Declared> {
    let at = |binding, class| Declared {
        set: entry.set,
        binding,
        class,
        stages,
    };
    let (first, second) = match class {
        Class::Sampler => (
            at(entry.sampler_binding.unwrap_or(entry.binding), class),
            None,
        ),
        Class::Texture { .. } => (
            at(entry.binding, class),
            entry
                .sampler_binding
                .map(|binding| at(binding, Class::Sampler)),
        ),
        _ => (at(entry.binding, class), None),
    };
    std::iter::once(first).chain(second)
}

/// Every resource `parameters` declares for the stage of `kind`, with duplicates merged.
///
/// A resource that names other stages only is kept when the table places it, so a module that
/// uses it can be told it belongs elsewhere, and is otherwise ignored, whatever it is.
pub(crate) fn declarations(
    parameters: &StageParameters,
    table: &[ResourceBinding],
    kind: StageKind,
) -> Result<Vec<Declared>, LayoutError> {
    let mut out = Vec::new();
    for resource in parameters.resources()? {
        let here = applies(resource.stages, kind);
        let class = match resource.class {
            Ok(class) => class,
            Err(error) if here => return Err(error),
            Err(_) => continue,
        };
        let found = match entry(table, resource.at) {
            Ok(found) => found,
            Err(error) if here => return Err(error),
            Err(_) => continue,
        };
        out.extend(placed(found, class, resource.stages));
    }
    out.sort_by_key(|d| (d.set, d.binding));
    Ok(merged(out))
}

/// Declarations of one resource, with the stages of each merged.
fn merged(sorted: Vec<Declared>) -> Vec<Declared> {
    let mut out: Vec<Declared> = Vec::new();
    for next in sorted {
        match out.last_mut() {
            Some(last)
                if (last.set, last.binding, last.class) == (next.set, next.binding, next.class) =>
            {
                last.stages = last.stages.zip(next.stages).map(|(a, b)| a | b);
            }
            _ => out.push(next),
        }
    }
    out
}

impl Class {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::ConstantBuffer => "a constant buffer",
            Self::Texture { .. } => "a texture",
            Self::Sampler => "a sampler",
            Self::StructuredBuffer => "a structured buffer",
            Self::UnorderedAccess => "a UAV",
            Self::StorageBuffer => "a buffer block",
            Self::StorageImage => "a storage image",
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
            | (Self::StorageImage, BindingType::StorageTexture { .. })
            | (
                Self::UnorderedAccess | Self::StorageBuffer,
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
            Self::UnorderedAccess | Self::StorageBuffer => ResourceKind::Storage {
                access: StorageAccess::ReadWrite,
                min_size: 0,
            },
            Self::StorageImage => ResourceKind::StorageImage {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu::TextureFormat::Rgba8Unorm,
                dimension: wgpu::TextureViewDimension::D2,
            },
        }
    }
}

/// The declaration a stage of `kind` reads at one address: the first that names the stage, or
/// else the first, which the stage then cannot use.
fn chosen(declared: &[Declared], set: u32, binding: u32, kind: StageKind) -> Option<&Declared> {
    let at_slot = || {
        declared
            .iter()
            .filter(|d| d.set == set && d.binding == binding)
    };
    at_slot()
        .find(|d| d.applies(kind))
        .or_else(|| at_slot().next())
}

/// The neutral declarations of a stage's Unity resources.
///
/// Unity's classes name a kind of resource but not a sample type, access or comparison
/// sampler, so each class that matches what the module declares at its address takes the
/// module's own full kind. Every resource the module uses is checked here, in the module's
/// order, as Unity names the failure: a resource no parameter declares, one that the
/// parameters bind for other stages only, and one whose class is not the module's kind.
///
/// A declaration is visible to the stage and to the stages its bits name. One that names other
/// stages only is not declared to this one, and so is not reported as unused.
pub(crate) fn resource_layout(
    declared: &[Declared],
    reflected: &[Reflected],
    kind: StageKind,
) -> Result<binding::ResourceLayout, LayoutError> {
    for resource in reflected {
        let found = chosen(declared, resource.set, resource.binding, kind).ok_or_else(|| {
            LayoutError::UndeclaredResource {
                set: resource.set,
                binding: resource.binding,
                name: resource.name.clone(),
            }
        })?;
        if !found.applies(kind) {
            return Err(LayoutError::StageMismatch {
                set: resource.set,
                binding: resource.binding,
                stage: kind,
                stages: found.stages.unwrap_or_default(),
            });
        }
        if !found.class.matches(&resource.ty) {
            return Err(LayoutError::Mismatch {
                set: resource.set,
                binding: resource.binding,
                declared: found.class.name(),
                found: format!("{:?}", resource.ty),
            });
        }
    }
    let mut bindings = Vec::new();
    for item in declared {
        let first = chosen(declared, item.set, item.binding, kind).is_some_and(|c| c == item);
        if !first || !item.applies(kind) {
            continue;
        }
        let used = reflected
            .iter()
            .find(|r| r.set == item.set && r.binding == item.binding);
        let resource_kind = used
            .and_then(|resource| ResourceKind::of_binding_type(&resource.ty, 0))
            .unwrap_or_else(|| item.class.unchecked_kind());
        bindings.push(binding::ResourceBinding {
            at: BindingAddress {
                set: item.set,
                binding: item.binding,
            },
            source: BindingSource::Unnamed,
            visibility: kind.visibility() | item.visibility(),
            kind: resource_kind,
        });
    }
    Ok(binding::ResourceLayout::new(bindings))
}
