//! Compute shaders, compute buffers, the values bound to a kernel and recorded dispatches.
//!
//! The world stores the shaders and buffers and records each dispatch with a snapshot of the
//! values it runs with; it runs nothing. A host executes the dispatch, writes the results into
//! the buffer's contents and the world hands them to whoever reads the buffer.
use super::properties::{check_name, upsert};
use super::{
    ComputeBufferId, ComputeShaderId, GraphicsError, OffscreenWork, PropertyValue,
    ShaderProperties, ShaderSource, TextureRef,
};
use crate::{World, WorldError};

/// Most kernels one compute shader holds.
const MAX_KERNELS: usize = 256;
/// Largest thread group dimension in x and y, and the largest product of the three.
const MAX_GROUP_XY: u32 = 1024;
/// Largest thread group dimension in z.
const MAX_GROUP_Z: u32 = 64;
/// Most thread groups a dispatch launches along one axis.
const MAX_DISPATCH_GROUPS: u32 = 65535;

/// Most bytes one compute buffer holds.
pub const MAX_COMPUTE_BUFFER_BYTES: u64 = 256 << 20;
/// Most bytes all of a world's compute buffers hold together.
pub const MAX_COMPUTE_TOTAL_BYTES: u64 = 1 << 30;
/// Largest element size of a compute buffer, in bytes.
const MAX_STRIDE: u32 = 2048;

/// One kernel of a compute shader. `thread_group_size` is `None` while nobody knows it: the
/// shader's data does not state it, so the host fills it in through
/// [`World::set_kernel_thread_group_size`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComputeKernel {
    pub name: String,
    pub thread_group_size: Option<[u32; 3]>,
}

/// A compute shader: where its programs are and its kernels, in the order a script addresses
/// them by index. Every edit raises `generation`; the one a caller passes in is ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldComputeShader {
    pub name: String,
    pub source: ShaderSource,
    pub kernels: Vec<ComputeKernel>,
    pub generation: u64,
}

/// A buffer of `count` elements of `stride` bytes. `kind` is the engine's buffer type value.
/// `contents` always holds `count * stride` bytes. Every write raises `generation`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorldComputeBuffer {
    pub count: u32,
    pub stride: u32,
    pub kind: u32,
    pub contents: Vec<u8>,
    pub generation: u64,
}

/// The values a kernel runs with: properties, in the shape materials use, and the compute
/// buffers bound by name, sorted by name. A texture property may name a global texture.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ComputeBindings {
    pub properties: ShaderProperties,
    pub buffers: Vec<(String, ComputeBufferId)>,
}

/// A compute shader with the values set on it.
pub(super) struct ComputeShaderEntry {
    shader: WorldComputeShader,
    /// Values set on the shader as a whole; every kernel sees them.
    shared: ShaderProperties,
    /// Values set on one kernel, which win over the shared ones.
    kernels: Vec<ComputeBindings>,
}

impl WorldComputeShader {
    fn validate(&self) -> Result<(), GraphicsError> {
        check_name(&self.name)?;
        self.source.validate()?;
        if self.kernels.len() > MAX_KERNELS {
            return Err(GraphicsError::InvalidComputeShader);
        }
        for kernel in &self.kernels {
            check_name(&kernel.name)?;
            if let Some(size) = kernel.thread_group_size {
                check_group_size(size)?;
            }
        }
        Ok(())
    }
}

fn check_group_size([x, y, z]: [u32; 3]) -> Result<(), GraphicsError> {
    let product = u64::from(x) * u64::from(y) * u64::from(z);
    let fits = (1..=MAX_GROUP_XY).contains(&x)
        && (1..=MAX_GROUP_XY).contains(&y)
        && (1..=MAX_GROUP_Z).contains(&z)
        && product <= u64::from(MAX_GROUP_XY);
    if fits {
        Ok(())
    } else {
        Err(GraphicsError::InvalidComputeShader)
    }
}

impl ComputeShaderEntry {
    fn kernel_mut(&mut self, kernel: u32) -> Result<&mut ComputeBindings, GraphicsError> {
        usize::try_from(kernel)
            .ok()
            .and_then(|index| self.kernels.get_mut(index))
            .ok_or(GraphicsError::InvalidKernel)
    }

    fn snapshot(&self, kernel: u32) -> Result<ComputeBindings, GraphicsError> {
        let own = usize::try_from(kernel)
            .ok()
            .and_then(|index| self.kernels.get(index))
            .ok_or(GraphicsError::InvalidKernel)?;
        Ok(ComputeBindings {
            properties: self.shared.merged_with(&own.properties),
            buffers: own.buffers.clone(),
        })
    }
}

fn byte_len(count: u32, stride: u32) -> u64 {
    u64::from(count) * u64::from(stride)
}

impl World {
    /// Registers a compute shader. The generation of `shader` is ignored: a new shader starts
    /// at 1. Every kernel has a name and, when it states a thread group size, one the hardware
    /// can launch: x and y of 1 to 1024, z of 1 to 64 and at most 1024 threads in all.
    pub fn create_compute_shader(
        &mut self,
        shader: WorldComputeShader,
    ) -> Result<ComputeShaderId, WorldError> {
        shader.validate()?;
        let entry = ComputeShaderEntry {
            kernels: vec![ComputeBindings::default(); shader.kernels.len()],
            shared: ShaderProperties::default(),
            shader: WorldComputeShader {
                generation: 1,
                ..shader
            },
        };
        Ok(ComputeShaderId(
            self.graphics.compute_shaders.insert(entry)?,
        ))
    }

    /// Removes a compute shader. Recorded dispatches that name it stay in the list and the host
    /// skips them once the id no longer resolves.
    pub fn destroy_compute_shader(&mut self, id: ComputeShaderId) -> Result<(), WorldError> {
        self.graphics
            .compute_shaders
            .remove(id.0)
            .map(drop)
            .ok_or(WorldError::Graphics(GraphicsError::StaleComputeShader))
    }

    pub fn compute_shader(&self, id: ComputeShaderId) -> Result<&WorldComputeShader, WorldError> {
        Ok(&self.compute_entry(id)?.shader)
    }

    /// Every live compute shader, in slot order.
    pub fn compute_shaders(&self) -> impl Iterator<Item = (ComputeShaderId, &WorldComputeShader)> {
        self.graphics
            .compute_shaders
            .iter()
            .map(|(bits, entry)| (ComputeShaderId(bits), &entry.shader))
    }

    fn compute_entry(&self, id: ComputeShaderId) -> Result<&ComputeShaderEntry, GraphicsError> {
        self.graphics
            .compute_shaders
            .get(id.0)
            .ok_or(GraphicsError::StaleComputeShader)
    }

    fn compute_entry_mut(
        &mut self,
        id: ComputeShaderId,
    ) -> Result<&mut ComputeShaderEntry, GraphicsError> {
        self.graphics
            .compute_shaders
            .get_mut(id.0)
            .ok_or(GraphicsError::StaleComputeShader)
    }

    /// Records the thread group size of a kernel whose size the shader's data did not state,
    /// or replaces the one it has, and raises the shader's generation.
    pub fn set_kernel_thread_group_size(
        &mut self,
        id: ComputeShaderId,
        kernel: u32,
        size: [u32; 3],
    ) -> Result<(), WorldError> {
        check_group_size(size)?;
        let entry = self.compute_entry_mut(id)?;
        let held = usize::try_from(kernel)
            .ok()
            .and_then(|index| entry.shader.kernels.get_mut(index))
            .ok_or(GraphicsError::InvalidKernel)?;
        held.thread_group_size = Some(size);
        entry.shader.generation += 1;
        Ok(())
    }

    /// Sets a value on one kernel, or on the whole shader when `kernel` is `None`, replacing the
    /// value of the same kind and name. A texture value may name a global texture.
    pub fn set_compute_property(
        &mut self,
        id: ComputeShaderId,
        kernel: Option<u32>,
        value: PropertyValue,
    ) -> Result<(), WorldError> {
        self.check_compute_value(&value)?;
        let entry = self.compute_entry_mut(id)?;
        let properties = match kernel {
            Some(kernel) => &mut entry.kernel_mut(kernel)?.properties,
            None => &mut entry.shared,
        };
        properties.set(value)?;
        Ok(())
    }

    fn check_compute_value(&self, value: &PropertyValue) -> Result<(), WorldError> {
        value.validate()?;
        match value {
            PropertyValue::Texture(_, slot) if slot.texture != TextureRef::None => {
                match &slot.texture {
                    TextureRef::Global(_) => Ok(()),
                    other => self.check_texture(other),
                }
            }
            _ => Ok(()),
        }
    }

    /// Binds a live compute buffer to a name of one kernel, replacing the buffer bound to it.
    pub fn bind_compute_buffer(
        &mut self,
        id: ComputeShaderId,
        kernel: u32,
        name: String,
        buffer: ComputeBufferId,
    ) -> Result<(), WorldError> {
        check_name(&name)?;
        self.compute_buffer(buffer)?;
        let bindings = self.compute_entry_mut(id)?.kernel_mut(kernel)?;
        upsert(&mut bindings.buffers, name, buffer)?;
        Ok(())
    }

    /// What a dispatch of `kernel` runs with now: the shader-wide values merged with the kernel's,
    /// the kernel's winning, and the kernel's buffers.
    pub fn compute_bindings(
        &self,
        id: ComputeShaderId,
        kernel: u32,
    ) -> Result<ComputeBindings, WorldError> {
        Ok(self.compute_entry(id)?.snapshot(kernel)?)
    }

    /// Records a dispatch of `kernel` over `groups` thread groups with a snapshot of the
    /// bindings it has now, in call order with the frame's other offscreen work. Later edits do
    /// not change the record. A group count is 1 to 65535 along each axis.
    pub fn dispatch_compute(
        &mut self,
        id: ComputeShaderId,
        kernel: u32,
        groups: [u32; 3],
    ) -> Result<(), WorldError> {
        let bindings = self.compute_bindings(id, kernel)?;
        self.record_offscreen(OffscreenWork::Dispatch {
            shader: id,
            kernel,
            groups,
            bindings,
        })
    }

    /// Checks the shader, the kernel and the group counts of a dispatch record.
    pub(super) fn check_dispatch(
        &self,
        shader: ComputeShaderId,
        kernel: u32,
        groups: [u32; 3],
    ) -> Result<(), WorldError> {
        let held = &self.compute_entry(shader)?.shader;
        if usize::try_from(kernel).map_or(true, |index| index >= held.kernels.len()) {
            return Err(GraphicsError::InvalidKernel.into());
        }
        if groups
            .iter()
            .all(|&count| (1..=MAX_DISPATCH_GROUPS).contains(&count))
        {
            Ok(())
        } else {
            Err(GraphicsError::InvalidOffscreen.into())
        }
    }

    /// Creates a buffer of `count` elements of `stride` bytes with zeroed contents. The stride
    /// is a multiple of 4 from 4 to 2048, the count is at least 1, one buffer holds at most
    /// 256 MiB and a world's buffers hold at most 1 GiB together.
    pub fn create_compute_buffer(
        &mut self,
        count: u32,
        stride: u32,
        kind: u32,
    ) -> Result<ComputeBufferId, WorldError> {
        let bytes = byte_len(count, stride);
        let stride_fits = (4..=MAX_STRIDE).contains(&stride) && stride.is_multiple_of(4);
        let total = self.graphics.compute_bytes.checked_add(bytes);
        let fits = count > 0
            && stride_fits
            && bytes <= MAX_COMPUTE_BUFFER_BYTES
            && total.is_some_and(|total| total <= MAX_COMPUTE_TOTAL_BYTES);
        if !fits {
            return Err(GraphicsError::InvalidComputeBuffer.into());
        }
        let length = usize::try_from(bytes).map_err(|_| GraphicsError::InvalidComputeBuffer)?;
        let id = self.graphics.compute_buffers.insert(WorldComputeBuffer {
            count,
            stride,
            kind,
            contents: vec![0; length],
            generation: 1,
        })?;
        self.graphics.compute_bytes += bytes;
        Ok(ComputeBufferId(id))
    }

    /// Removes a compute buffer and frees its contents. Kernels and recorded dispatches that
    /// name it keep the stale id.
    pub fn destroy_compute_buffer(&mut self, id: ComputeBufferId) -> Result<(), WorldError> {
        let removed = self
            .graphics
            .compute_buffers
            .remove(id.0)
            .ok_or(GraphicsError::StaleComputeBuffer)?;
        self.graphics.compute_bytes -= byte_len(removed.count, removed.stride);
        Ok(())
    }

    pub fn compute_buffer(&self, id: ComputeBufferId) -> Result<&WorldComputeBuffer, WorldError> {
        Ok(self
            .graphics
            .compute_buffers
            .get(id.0)
            .ok_or(GraphicsError::StaleComputeBuffer)?)
    }

    /// Every live compute buffer, in slot order.
    pub fn compute_buffers(&self) -> impl Iterator<Item = (ComputeBufferId, &WorldComputeBuffer)> {
        self.graphics
            .compute_buffers
            .iter()
            .map(|(bits, buffer)| (ComputeBufferId(bits), buffer))
    }

    /// Writes `bytes` into the buffer's contents at `byte_offset` and raises its generation. The
    /// range must lie inside the buffer; a rejected write changes nothing.
    pub fn set_compute_buffer_data(
        &mut self,
        id: ComputeBufferId,
        byte_offset: usize,
        bytes: &[u8],
    ) -> Result<(), WorldError> {
        let buffer = self
            .graphics
            .compute_buffers
            .get_mut(id.0)
            .ok_or(GraphicsError::StaleComputeBuffer)?;
        let end = byte_offset
            .checked_add(bytes.len())
            .filter(|&end| end <= buffer.contents.len())
            .ok_or(GraphicsError::ComputeRange)?;
        buffer.contents[byte_offset..end].copy_from_slice(bytes);
        buffer.generation += 1;
        Ok(())
    }
}
