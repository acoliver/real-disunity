//! The resources a stage's Unity parameters declare, placed through its binding table.

use wgpu::ShaderStages;
use worldrunner_unity_serialized::shader_parameters::DescriptorSlot;

use crate::layout::LayoutError;
use crate::module::{ResourceBinding, StageKind, StageParameters};
use crate::source::{At, Source};

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
    pub(crate) fn visibility(&self) -> ShaderStages {
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
    match (stages, kind.stage_bit()) {
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
