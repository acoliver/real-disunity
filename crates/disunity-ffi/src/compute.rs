//! Compute shaders, compute buffers, the values bound to a kernel and recorded dispatches. The
//! rules live in world-simulation; this file validates strings and records and forwards.
use crate::{
    DisunityResult,
    callbacks::mutate,
    errors::AbiError,
    graphics::text,
    graphics_properties::{DisunityTextureSlot, texture_ref},
};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, ComputeKernel, ComputeShaderId, PropertyValue, ShaderSource, TextureRef,
    TextureSlot, WorldComputeShader,
};

/// Most bytes of names a compute shader is created from or read into: its name, its file and
/// its kernel names.
const MAX_BLOB_BYTES: u32 = 1 << 20;
/// Most kernels one call describes.
const MAX_KERNELS: u32 = 256;

/// A compute shader to create. `text` holds the name (`name_length` bytes), then the file
/// (`file_length` bytes), then whatever the kernels name; `kernel_count` records describe the
/// kernels.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeShaderDesc {
    pub path_id: i64,
    pub name_length: u32,
    pub file_length: u32,
    pub kernel_count: u32,
}

/// One kernel: its name is `name_length` bytes of the text at `name_offset`. A thread group size
/// of 0 by 0 by 0 is unknown; any other size must be launchable.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeKernel {
    pub name_offset: u32,
    pub name_length: u32,
    pub size_x: u32,
    pub size_y: u32,
    pub size_z: u32,
}

/// What a compute shader holds. On input, `kernel_count` and `text_length` are the capacities of
/// the arrays a caller passes; on output they are what the shader holds. `generation` rises with
/// every edit.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeShaderInfo {
    pub path_id: i64,
    pub name_length: u32,
    pub file_length: u32,
    pub kernel_count: u32,
    pub text_length: u32,
    pub generation: u64,
}

/// A compute buffer to create: `count` elements of `stride` bytes, `kind` the engine's type.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBufferDesc {
    pub count: u32,
    pub stride: u32,
    pub kind: u32,
}

/// What a compute buffer holds besides its contents. `generation` rises with every write.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBufferInfo {
    pub count: u32,
    pub stride: u32,
    pub kind: u32,
    pub generation: u64,
}

/// The lengths of a texture property's name and of the global texture it names, which are the
/// first and second string of the text beside the record.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeGlobalTexture {
    pub name_length: u32,
    pub global_length: u32,
}

/// The ABI sentinel for values that belong to the shader as a whole, not to one kernel.
const SHADER_WIDE: i32 = -1;

fn shader_id(bits: u64) -> ComputeShaderId {
    ComputeShaderId::from_bits(bits)
}

/// A slice of the blob of names. The range must lie inside it.
fn slice(blob: &str, offset: u32, length: u32) -> Result<&str, AbiError> {
    let start = offset as usize;
    let end = start
        .checked_add(length as usize)
        .ok_or("a name lies outside the text")?;
    blob.get(start..end)
        .ok_or_else(|| "a name lies outside the text or splits a character".into())
}

/// The size of a kernel the record states: none when it is all zero.
fn kernel_size(kernel: &DisunityComputeKernel) -> Option<[u32; 3]> {
    let size = [kernel.size_x, kernel.size_y, kernel.size_z];
    (size != [0; 3]).then_some(size)
}

fn build_shader(
    desc: &DisunityComputeShaderDesc,
    blob: &str,
    kernels: &[DisunityComputeKernel],
) -> Result<WorldComputeShader, AbiError> {
    Ok(WorldComputeShader {
        name: slice(blob, 0, desc.name_length)?.to_owned(),
        source: ShaderSource {
            file: slice(blob, desc.name_length, desc.file_length)?.to_owned(),
            path_id: desc.path_id,
        },
        kernels: kernels
            .iter()
            .map(|kernel| {
                Ok(ComputeKernel {
                    name: slice(blob, kernel.name_offset, kernel.name_length)?.to_owned(),
                    thread_group_size: kernel_size(kernel),
                })
            })
            .collect::<Result<_, AbiError>>()?,
        generation: 0,
    })
}

/// Registers a compute shader and writes its id.
///
/// # Safety
/// `desc` is readable for one record; `text` spans `text_length` readable bytes; `kernels` is
/// readable for `desc.kernel_count` records, or null when that is zero; `output` is writable for
/// one id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_shader_create(
    world: u64,
    desc: *const DisunityComputeShaderDesc,
    text: *const u8,
    text_length: u32,
    kernels: *const DisunityComputeKernel,
    output: *mut u64,
) -> DisunityResult {
    if desc.is_null() || output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = unsafe { *desc };
    if desc.kernel_count > MAX_KERNELS
        || (desc.kernel_count > 0 && kernels.is_null())
        || text.is_null()
        || text_length == 0
        || text_length > MAX_BLOB_BYTES
    {
        return DisunityResult::invalid_argument();
    }
    let kernels: &[DisunityComputeKernel] = if desc.kernel_count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(kernels, desc.kernel_count as usize) }
    };
    mutate(world, |s| {
        let bytes = unsafe { std::slice::from_raw_parts(text, text_length as usize) };
        let blob = std::str::from_utf8(bytes).map_err(|_| "names must be UTF-8")?;
        let id = s
            .native
            .create_compute_shader(build_shader(&desc, blob, kernels)?)?;
        unsafe { *output = id.to_bits() };
        Ok(())
    })
}

/// Removes a compute shader. Dispatches already recorded stay and the host skips them.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_compute_shader_destroy(world: u64, shader: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native.destroy_compute_shader(shader_id(shader))?;
        Ok(())
    })
}

/// Reads a compute shader. With null `kernels` and `text` only `info` is filled in, to size the
/// arrays; otherwise the capacities in `info` must hold what the shader has, and the arrays are
/// filled: the text is the name, the file and then the kernel names.
///
/// # Safety
/// `info` is readable and writable for one record; `kernels` is writable for the capacity
/// `info` states and `text` for its text capacity, or both are null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_shader_read(
    world: u64,
    shader: u64,
    info: *mut DisunityComputeShaderInfo,
    kernels: *mut DisunityComputeKernel,
    text: *mut u8,
) -> DisunityResult {
    if info.is_null() || kernels.is_null() != text.is_null() {
        return DisunityResult::invalid_argument();
    }
    let capacity = unsafe { *info };
    mutate(world, |s| {
        let held = s.native.compute_shader(shader_id(shader))?;
        let mut blob = format!("{}{}", held.name, held.source.file);
        let mut records = Vec::with_capacity(held.kernels.len());
        for kernel in &held.kernels {
            let size = kernel.thread_group_size.unwrap_or([0; 3]);
            records.push(DisunityComputeKernel {
                name_offset: u32::try_from(blob.len()).map_err(|_| "text overflow")?,
                name_length: u32::try_from(kernel.name.len()).map_err(|_| "text overflow")?,
                size_x: size[0],
                size_y: size[1],
                size_z: size[2],
            });
            blob.push_str(&kernel.name);
        }
        let reported = DisunityComputeShaderInfo {
            path_id: held.source.path_id,
            name_length: u32::try_from(held.name.len()).map_err(|_| "text overflow")?,
            file_length: u32::try_from(held.source.file.len()).map_err(|_| "text overflow")?,
            kernel_count: u32::try_from(records.len()).map_err(|_| "count overflow")?,
            text_length: u32::try_from(blob.len()).map_err(|_| "text overflow")?,
            generation: held.generation,
        };
        if !kernels.is_null() {
            if capacity.kernel_count < reported.kernel_count
                || capacity.text_length < reported.text_length
            {
                return Err("the kernel or text capacity is too small".into());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(records.as_ptr(), kernels, records.len());
                std::ptr::copy_nonoverlapping(blob.as_ptr(), text, blob.len());
            }
        }
        unsafe { *info = reported };
        Ok(())
    })
}

/// Records the thread group size of a kernel whose size the shader's data did not state.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_compute_kernel_size_write(
    world: u64,
    shader: u64,
    kernel: u32,
    x: u32,
    y: u32,
    z: u32,
) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .set_kernel_thread_group_size(shader_id(shader), kernel, [x, y, z])?;
        Ok(())
    })
}

/// Creates a compute buffer with zeroed contents and writes its id.
///
/// # Safety
/// `desc` is readable for one record; `output` is writable for one id.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_buffer_create(
    world: u64,
    desc: *const DisunityComputeBufferDesc,
    output: *mut u64,
) -> DisunityResult {
    if desc.is_null() || output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = unsafe { *desc };
    mutate(world, |s| {
        let id = s
            .native
            .create_compute_buffer(desc.count, desc.stride, desc.kind)?;
        unsafe { *output = id.to_bits() };
        Ok(())
    })
}

/// Removes a compute buffer and frees its contents.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_compute_buffer_destroy(world: u64, buffer: u64) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .destroy_compute_buffer(ComputeBufferId::from_bits(buffer))?;
        Ok(())
    })
}

/// # Safety
/// `output` is writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_buffer_info(
    world: u64,
    buffer: u64,
    output: *mut DisunityComputeBufferInfo,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let held = s
            .native
            .compute_buffer(ComputeBufferId::from_bits(buffer))?;
        unsafe {
            *output = DisunityComputeBufferInfo {
                count: held.count,
                stride: held.stride,
                kind: held.kind,
                generation: held.generation,
            };
        }
        Ok(())
    })
}

fn offset(value: u64) -> Result<usize, AbiError> {
    usize::try_from(value).map_err(|_| "the offset is outside the buffer".into())
}

/// Writes `length` bytes into a compute buffer at `byte_offset`. The range must lie inside the
/// buffer; a rejected call changes nothing.
///
/// # Safety
/// `data` is readable for `length` bytes, or null when `length` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_buffer_write(
    world: u64,
    buffer: u64,
    byte_offset: u64,
    data: *const u8,
    length: u32,
) -> DisunityResult {
    if length > 0 && data.is_null() {
        return DisunityResult::invalid_argument();
    }
    let bytes: &[u8] = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(data, length as usize) }
    };
    mutate(world, |s| {
        s.native.set_compute_buffer_data(
            ComputeBufferId::from_bits(buffer),
            offset(byte_offset)?,
            bytes,
        )?;
        Ok(())
    })
}

/// Reads `length` bytes of a compute buffer from `byte_offset`. The range must lie inside the
/// buffer.
///
/// # Safety
/// `output` is writable for `length` bytes, or null when `length` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_buffer_read(
    world: u64,
    buffer: u64,
    byte_offset: u64,
    output: *mut u8,
    length: u32,
) -> DisunityResult {
    if length > 0 && output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let held = s
            .native
            .compute_buffer(ComputeBufferId::from_bits(buffer))?;
        let start = offset(byte_offset)?;
        let end = start
            .checked_add(length as usize)
            .filter(|&end| end <= held.contents.len())
            .ok_or("the range lies outside the compute buffer")?;
        if length > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    held.contents[start..end].as_ptr(),
                    output,
                    length as usize,
                );
            }
        }
        Ok(())
    })
}

/// Which kernel a value belongs to: one kernel, or the shader as a whole for -1.
fn kernel_scope(kernel: i32) -> Result<Option<u32>, AbiError> {
    if kernel == SHADER_WIDE {
        return Ok(None);
    }
    u32::try_from(kernel)
        .map(Some)
        .map_err(|_| "a kernel is its index, or -1 for the whole shader".into())
}

fn set_value(
    world: u64,
    shader: u64,
    kernel: i32,
    make: impl FnOnce() -> Result<PropertyValue, AbiError>,
) -> DisunityResult {
    mutate(world, |s| {
        let kernel = kernel_scope(kernel)?;
        s.native
            .set_compute_property(shader_id(shader), kernel, make()?)?;
        Ok(())
    })
}

/// Sets a float on one kernel, or on the whole shader when `kernel` is -1.
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_float_write(
    world: u64,
    shader: u64,
    kernel: i32,
    name: *const u8,
    length: u32,
    value: f32,
) -> DisunityResult {
    set_value(world, shader, kernel, || {
        Ok(PropertyValue::Float(unsafe { text(name, length) }?, value))
    })
}

/// Sets an integer on one kernel, or on the whole shader when `kernel` is -1.
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_int_write(
    world: u64,
    shader: u64,
    kernel: i32,
    name: *const u8,
    length: u32,
    value: i32,
) -> DisunityResult {
    set_value(world, shader, kernel, || {
        Ok(PropertyValue::Int(unsafe { text(name, length) }?, value))
    })
}

/// Sets a vector from 4 floats.
///
/// # Safety
/// `name` spans `length` readable bytes; `values` is readable for 4 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_vector_write(
    world: u64,
    shader: u64,
    kernel: i32,
    name: *const u8,
    length: u32,
    values: *const f32,
) -> DisunityResult {
    set_value(world, shader, kernel, || {
        if values.is_null() {
            return Err("a vector needs 4 floats".into());
        }
        let mut vector = [0.0; 4];
        unsafe { std::ptr::copy_nonoverlapping(values, vector.as_mut_ptr(), vector.len()) };
        Ok(PropertyValue::Vector(
            unsafe { text(name, length) }?,
            vector,
        ))
    })
}

/// Sets a matrix from 16 floats in column-major order.
///
/// # Safety
/// `name` spans `length` readable bytes; `values` is readable for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_matrix_write(
    world: u64,
    shader: u64,
    kernel: i32,
    name: *const u8,
    length: u32,
    values: *const f32,
) -> DisunityResult {
    set_value(world, shader, kernel, || {
        if values.is_null() {
            return Err("a matrix needs 16 floats".into());
        }
        let mut matrix = [0.0; 16];
        unsafe { std::ptr::copy_nonoverlapping(values, matrix.as_mut_ptr(), matrix.len()) };
        Ok(PropertyValue::Matrix(
            unsafe { text(name, length) }?,
            matrix,
        ))
    })
}

/// Sets a texture of a kernel by what a texture property names: none, a player texture or a
/// render texture.
///
/// # Safety
/// `name` spans `length` readable bytes; `slot` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_texture_write(
    world: u64,
    shader: u64,
    kernel: i32,
    name: *const u8,
    length: u32,
    slot: *const DisunityTextureSlot,
) -> DisunityResult {
    set_value(world, shader, kernel, || {
        if slot.is_null() {
            return Err("a texture needs a slot record".into());
        }
        let slot = unsafe { *slot };
        Ok(PropertyValue::Texture(
            unsafe { text(name, length) }?,
            TextureSlot {
                texture: texture_ref(slot.kind, slot.texture)?,
                scale: [slot.scale_x, slot.scale_y],
                offset: [slot.offset_x, slot.offset_y],
            },
        ))
    })
}

/// Sets a texture of a kernel to the global texture of the given name, with scale one and offset
/// zero. `names` holds the texture property's name, then the global texture's name, with the
/// lengths in `lengths`.
///
/// # Safety
/// `lengths` is readable for one record; `names` spans the two lengths together.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_texture_global_write(
    world: u64,
    shader: u64,
    kernel: i32,
    lengths: *const DisunityComputeGlobalTexture,
    names: *const u8,
) -> DisunityResult {
    if lengths.is_null() {
        return DisunityResult::invalid_argument();
    }
    let lengths = unsafe { *lengths };
    set_value(world, shader, kernel, || {
        let name = unsafe { text(names, lengths.name_length) }?;
        let rest = names.wrapping_add(lengths.name_length as usize);
        let global = unsafe { text(rest, lengths.global_length) }?;
        Ok(PropertyValue::Texture(
            name,
            TextureSlot {
                texture: TextureRef::Global(global),
                ..TextureSlot::default()
            },
        ))
    })
}

/// Binds a live compute buffer to a name of one kernel.
///
/// # Safety
/// `name` spans `length` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_buffer_bind(
    world: u64,
    shader: u64,
    kernel: u32,
    name: *const u8,
    length: u32,
    buffer: u64,
) -> DisunityResult {
    mutate(world, |s| {
        let name = unsafe { text(name, length) }?;
        s.native.bind_compute_buffer(
            shader_id(shader),
            kernel,
            name,
            ComputeBufferId::from_bits(buffer),
        )?;
        Ok(())
    })
}

/// Records a dispatch of a kernel over a number of thread groups, with a snapshot of the values
/// bound to the kernel now.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_compute_dispatch(
    world: u64,
    shader: u64,
    kernel: u32,
    groups_x: u32,
    groups_y: u32,
    groups_z: u32,
) -> DisunityResult {
    mutate(world, |s| {
        s.native
            .dispatch_compute(shader_id(shader), kernel, [groups_x, groups_y, groups_z])?;
        Ok(())
    })
}
