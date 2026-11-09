//! A compute kernel decoded from a player's compute shader, as a stage program.
//!
//! A variant's bindings become the [`VariantParameters`] a graphics variant stores for the same
//! kinds of resource, so layouts, targets and packing treat both alike:
//!
//! - A constant buffer, texture, buffer or writable buffer binds at the descriptor set and
//!   binding its slot gives. A writable buffer is a buffer, as the layout decides its access from
//!   the module. A storage image is a storage image, and the layout takes its access, format and
//!   dimension from the module.
//! - A texture's dimension, which counts as 2 for a 2D texture, 3 for a 3D one and 5 for a 2D
//!   array, becomes the code a graphics variant uses, twice the number.
//! - A texture's separate sampler is bound at the texture's set and the binding the texture
//!   states; a built-in sampler has no name and binds at set 0.
//! - A constant buffer's layout is the platform's layout of the same name, whose members are
//!   scalars, vectors and matrices; a member with more than one row is a matrix.
//!
//! No bit of a compute slot names a stage, so every binding carries none and applies to the
//! compute stage.

use worldrunner_unity_serialized::SerializedError;
use worldrunner_unity_serialized::compute_shader::{
    ComputeBinding, ComputeBuffer, ComputeMember, ComputeVariant,
};
use worldrunner_unity_serialized::shader_parameters::{
    BufferMember, ConstantBuffer, DescriptorSlot, ResourceBinding, ResourceKind, VariantParameters,
};
use worldrunner_unity_serialized::spirv_prep::{PrepareError, prepare};

use crate::module::entry_point_names;

use super::module::{ModuleError, StageKind, StageModule, StageProgram};

#[derive(Debug, thiserror::Error)]
pub enum KernelError {
    #[error("the kernel's code is not a SPIR-V module: {0}")]
    Code(SerializedError),
    #[error("the kernel's module cannot be prepared: {0}")]
    Prepare(#[from] PrepareError),
    #[error("the kernel's module has {0} entry points, not one")]
    EntryPoints(usize),
    #[error("{0}")]
    Module(#[from] ModuleError),
    #[error("the texture {name:?} states no dimension")]
    NoDimension { name: String },
}

fn slot_of(binding: &ComputeBinding) -> DescriptorSlot {
    DescriptorSlot {
        binding: binding.slot.binding(),
        set: binding.slot.set(),
        stages: 0,
    }
}

fn resource(binding: &ComputeBinding, kind: ResourceKind) -> ResourceBinding {
    ResourceBinding {
        name: binding.name.clone(),
        slot: slot_of(binding),
        kind,
    }
}

fn texture(binding: &ComputeBinding) -> Result<ResourceBinding, KernelError> {
    let slot = slot_of(binding);
    let dimension = binding.dimension.ok_or_else(|| KernelError::NoDimension {
        name: binding.name.clone(),
    })?;
    // The separate sampler is bound in the texture's set, at the binding the texture states.
    let sampler = binding
        .sampler
        .and_then(|bind_point| u16::try_from(bind_point).ok())
        .map(|binding| DescriptorSlot { binding, ..slot });
    Ok(resource(
        binding,
        ResourceKind::Texture {
            sampler,
            dimension: dimension.saturating_mul(2),
        },
    ))
}

/// A writable binding is a storage image when it states a dimension and a buffer otherwise.
fn writable(binding: &ComputeBinding) -> ResourceBinding {
    let kind = if binding.slot.is_image() {
        ResourceKind::StorageImage
    } else {
        ResourceKind::Buffer
    };
    resource(binding, kind)
}

fn member(member: &ComputeMember) -> BufferMember {
    BufferMember {
        name: member.name.clone(),
        kind: member.kind,
        rows: member.rows,
        columns: member.columns,
        is_matrix: member.rows > 1,
        array_size: member.array_size,
        offset: member.offset,
    }
}

fn layout(buffer: &ComputeBuffer) -> ConstantBuffer {
    ConstantBuffer {
        name: buffer.name.clone(),
        size: i32::try_from(buffer.size).unwrap_or(i32::MAX),
        members: buffer.members.iter().map(member).collect(),
        structs: Vec::new(),
    }
}

/// The parameters a graphics variant would store for `variant`, with the constant buffer layouts
/// of its platform.
///
/// # Errors
/// Fails when a texture states no dimension.
pub fn variant_parameters(
    variant: &ComputeVariant,
    buffers: &[ComputeBuffer],
) -> Result<VariantParameters, KernelError> {
    let mut bindings: Vec<ResourceBinding> = variant
        .constant_buffers
        .iter()
        .map(|binding| resource(binding, ResourceKind::ConstantBuffer))
        .collect();
    for binding in &variant.textures {
        bindings.push(texture(binding)?);
    }
    bindings.extend(variant.samplers.iter().map(|sampler| ResourceBinding {
        name: String::new(),
        slot: DescriptorSlot {
            binding: u16::try_from(sampler.bind_point).unwrap_or(u16::MAX),
            set: 0,
            stages: 0,
        },
        kind: ResourceKind::Sampler {
            state: sampler.state,
        },
    }));
    bindings.extend(
        variant
            .buffers
            .iter()
            .map(|binding| resource(binding, ResourceKind::Buffer)),
    );
    bindings.extend(variant.writable.iter().map(writable));
    Ok(VariantParameters {
        constant_buffers: buffers.iter().map(layout).collect(),
        bindings,
    })
}

/// The compute stage of `variant`: its module prepared for naga, its one entry point and the
/// parameters of [`variant_parameters`].
///
/// # Errors
/// Fails when the code is not a SPIR-V module, the module cannot be prepared or parsed, it does
/// not have exactly one entry point, or a texture states no dimension.
pub fn kernel_stage(
    variant: &ComputeVariant,
    buffers: &[ComputeBuffer],
) -> Result<StageProgram, KernelError> {
    let parameters = variant_parameters(variant, buffers)?;
    let words = variant.module_words().map_err(KernelError::Code)?;
    let prepared = prepare(&words)?;
    let mut names = entry_point_names(&prepared.words);
    if names.len() != 1 {
        return Err(KernelError::EntryPoints(names.len()));
    }
    let module = StageModule {
        entry_point: names.remove(0),
        words: prepared.words,
        kind: StageKind::Compute,
        bindings: prepared.bindings,
    };
    Ok(StageProgram::new(module, parameters)?)
}
