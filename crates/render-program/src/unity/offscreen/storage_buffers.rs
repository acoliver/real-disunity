//! The read-only structured buffers a graphics draw reads: Unity's `StructuredBuffer<T>` of a
//! material, which the shader names by its HLSL name (`_Properties`) and a host sets with
//! `Material.SetBuffer`.
//!
//! A slot is found by the same rule a kernel's buffers are: the stage's parameters name the
//! buffer at a prepared set and binding ([`BufferTarget`](crate::unity::targets::BufferTarget)),
//! and the layers are asked for that name in the order they are asked for a texture's. The layer
//! holds the [`BufferHandle`] a compute dispatch binds, so one uploaded buffer serves a dispatch
//! and a draw. The draw binds the whole buffer, read only, to the stages the slot declares.
//!
//! Everything a slot can lack is found before the draw binds anything: a buffer no layer names
//! leaves the draw out and counts it by name, as an unset texture does, and a slot the draw
//! cannot bind (a writable one, or one more than a stage of the device binds) refuses the frame by
//! name.

use worldrunner_render_post::contracts::{BufferView, ResourceLifetime};

use crate::layout::Slot;
use crate::resources::BufferHandle;
use crate::unity::targets::BufferTarget;

use super::error::OffscreenError;
use super::plan::{DrawPlan, DrawSpec, ProgramEntry, Services};

/// A read-only storage buffer binding of a draw, over the whole of its buffer.
pub(super) struct PendingStorage {
    pub(super) set: u32,
    pub(super) binding: u32,
    pub(super) visibility: wgpu::ShaderStages,
    pub(super) view: BufferView,
}

/// A slot and the buffer the layers give it, found before the draw binds.
pub(super) struct StorageInput {
    set: u32,
    binding: u32,
    visibility: wgpu::ShaderStages,
    min_size: u64,
    name: String,
    buffer: BufferHandle,
}

/// The refusal of the buffer `name` at set `set` and `binding`.
fn refused(name: &str, (set, binding): (u32, u32), why: String) -> OffscreenError {
    OffscreenError::UnsupportedBuffer {
        name: name.to_owned(),
        set,
        binding,
        why,
    }
}

/// Checks that the device binds as many storage buffers to each stage as the stage's parameters
/// declare, before the program is made: a device that binds none to a stage (the adapters whose
/// vertex stage has no storage buffers) refuses the layout, and its own message names no property.
///
/// # Errors
/// Fails by name with the first buffer of a stage that the device cannot bind.
pub(super) fn check_stage_limits(
    limit: u32,
    stages: [(&str, &[BufferTarget]); 2],
) -> Result<(), OffscreenError> {
    let limit = usize::try_from(limit).unwrap_or(usize::MAX);
    for (stage, targets) in stages {
        if let Some(target) = targets.get(limit) {
            return Err(refused(
                target.name.resolve(&[])?,
                (target.set, target.binding),
                format!(
                    "is read by the {stage} stage, and the device binds at most {limit} storage \
                     buffers to a stage"
                ),
            ));
        }
    }
    Ok(())
}

/// The buffer each storage slot of the program reads, found in the layers by the slot's
/// property name.
///
/// # Errors
/// Fails with `MissingBuffer` for a slot no layer names a buffer for, and by name for a slot
/// that is writable.
pub(super) fn require_buffers(
    spec: &DrawSpec<'_>,
    entry: &ProgramEntry,
    slots: &[Slot],
) -> Result<Vec<StorageInput>, OffscreenError> {
    let mut inputs = Vec::new();
    for slot in slots {
        let wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            ..
        } = slot.ty
        else {
            continue;
        };
        let name = entry
            .buffer_name(slot)?
            .ok_or(OffscreenError::UnsupportedBinding {
                set: slot.set,
                binding: slot.binding,
                what: if read_only {
                    "a storage buffer no parameter names"
                } else {
                    "a writable storage buffer"
                },
            })?;
        let at = (slot.set, slot.binding);
        if !read_only {
            return Err(refused(
                name,
                at,
                "is a writable storage buffer, which offscreen draws do not bind".to_owned(),
            ));
        }
        let buffer = spec
            .dots
            .and_then(|dots| dots.buffer(name))
            .or_else(|| spec.layers.buffer(name))
            .ok_or_else(|| OffscreenError::MissingBuffer {
                name: name.to_owned(),
            })?;
        inputs.push(StorageInput {
            set: slot.set,
            binding: slot.binding,
            visibility: slot.visibility,
            min_size: slot.min_size,
            name: name.to_owned(),
            buffer,
        });
    }
    Ok(inputs)
}

impl Services<'_> {
    /// Binds the buffer of `input`, whole and read only, to the stages its slot declares.
    pub(super) fn bind_storage(
        &mut self,
        input: &StorageInput,
        plan: &mut DrawPlan,
    ) -> Result<(), OffscreenError> {
        self.shares
            .push(self.runtime.store.buffer_reservation(input.buffer)?);
        let buffer = self.runtime.gpu_buffer(input.buffer)?;
        let len = buffer.size();
        if len < input.min_size {
            return Err(refused(
                &input.name,
                (input.set, input.binding),
                format!("holds {len} bytes and the shader reads {}", input.min_size),
            ));
        }
        let key =
            self.tracker
                .import_buffer(&buffer, ResourceLifetime::ImportedReadOnly, Some(0))?;
        let view = self.tracker.buffer_view(key, 0..len, 0)?;
        plan.storage.push(PendingStorage {
            set: input.set,
            binding: input.binding,
            visibility: input.visibility,
            view,
        });
        Ok(())
    }
}
