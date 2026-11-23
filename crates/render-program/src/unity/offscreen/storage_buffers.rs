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

use std::ops::Range;

use worldrunner_render_post::contracts::{BufferView, ResourceLifetime};

use crate::layout::Slot;
use crate::resources::BufferHandle;
use crate::unity::targets::BufferTarget;

use super::defaults::DefaultBuffers;
use super::dots::DotsDraw;
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
    /// The bytes of the buffer the slot reads, or `None` for the whole of it.
    window: Option<Range<u64>>,
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

/// The buffer slot `name` reads and the bytes of it, in the order a DOTS batch, the layers and a
/// DOTS draw's placeholder are asked.
fn source(
    spec: &DrawSpec<'_>,
    name: &str,
    defaults: DefaultBuffers,
) -> Option<(BufferHandle, Option<Range<u64>>)> {
    let from_batch = spec
        .dots
        .and_then(|dots| dots.buffer(name))
        .map(|(buffer, window)| (buffer, Some(window)));
    from_batch
        .or_else(|| spec.layers.buffer(name).map(|buffer| (buffer, None)))
        .or_else(|| {
            spec.dots?;
            let placeholder = DotsDraw::placeholder(name, defaults)?;
            Some((placeholder, None))
        })
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
    defaults: DefaultBuffers,
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
        let (buffer, window) =
            source(spec, name, defaults).ok_or_else(|| OffscreenError::MissingBuffer {
                name: name.to_owned(),
            })?;
        inputs.push(StorageInput {
            set: slot.set,
            binding: slot.binding,
            visibility: slot.visibility,
            min_size: slot.min_size,
            name: name.to_owned(),
            buffer,
            window,
        });
    }
    Ok(inputs)
}

impl Services<'_> {
    /// Binds the window of the buffer of `input`, read only, to the stages its slot declares.
    pub(super) fn bind_storage(
        &mut self,
        input: &StorageInput,
        plan: &mut DrawPlan,
    ) -> Result<(), OffscreenError> {
        self.shares
            .push(self.runtime.store.buffer_reservation(input.buffer)?);
        let buffer = self.runtime.gpu_buffer(input.buffer)?;
        let len = buffer.size();
        let range = self.window_of(input, len)?;
        let key =
            self.tracker
                .import_buffer(&buffer, ResourceLifetime::ImportedReadOnly, Some(0))?;
        let view = self.tracker.buffer_view(key, range, 0)?;
        plan.storage.push(PendingStorage {
            set: input.set,
            binding: input.binding,
            visibility: input.visibility,
            view,
        });
        Ok(())
    }

    /// The bytes of a buffer of `len` bytes that `input`'s slot binds, which the slot's shader
    /// can read in full and a device can bind where they start.
    fn window_of(&self, input: &StorageInput, len: u64) -> Result<Range<u64>, OffscreenError> {
        let at = (input.set, input.binding);
        let range = input.window.clone().unwrap_or(0..len);
        if range.start > range.end || range.end > len {
            return Err(refused(
                &input.name,
                at,
                format!("is read over bytes {range:?} of a buffer of {len} bytes"),
            ));
        }
        let alignment = u64::from(
            self.runtime
                .device()
                .limits()
                .min_storage_buffer_offset_alignment,
        );
        if alignment != 0 && !range.start.is_multiple_of(alignment) {
            return Err(refused(
                &input.name,
                at,
                format!(
                    "is read from byte {}, and the device binds storage buffers at multiples \
                     of {alignment} bytes",
                    range.start
                ),
            ));
        }
        let held = range.end - range.start;
        if held < input.min_size {
            return Err(refused(
                &input.name,
                at,
                format!("holds {held} bytes and the shader reads {}", input.min_size),
            ));
        }
        Ok(range)
    }
}
