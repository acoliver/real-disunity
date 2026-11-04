//! The values a recorded dispatch ran with, read by the host that executes it.
//!
//! Taking the frame's offscreen work (see [`crate::graphics_offscreen`]) keeps each dispatch's
//! bindings in the session and gives the record a token, from 1, naming them. A token is valid
//! until the next call that takes work.
use crate::{
    DisunityResult, callbacks::mutate, errors::AbiError, graphics_properties::texture_parts,
};
use worldrunner_world_simulation::graphics::{ComputeBindings, TextureRef};

const KIND_FLOAT: u32 = 0;
const KIND_INT: u32 = 1;
const KIND_VECTOR: u32 = 2;
const KIND_MATRIX: u32 = 3;
const KIND_TEXTURE: u32 = 4;
const KIND_BUFFER: u32 = 5;

/// One bound value. `kind` is 0 for a float (one float at `float_offset`), 1 for an integer
/// (`int_value`), 2 for a vector (four floats), 3 for a matrix (sixteen floats, column-major),
/// 4 for a texture and 5 for a buffer (`resource` is the compute buffer's id). A texture reads
/// scale x, scale y, offset x and offset y at `float_offset`; its `texture_kind` is 0 for none,
/// 1 for a player texture, 2 for a render texture (`resource` is the identity or the id) and
/// 3 for a global texture, named by `global_length` bytes of the text at `global_offset`. The
/// name is `name_length` bytes of the text at `name_offset`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBinding {
    pub kind: u32,
    pub name_offset: u32,
    pub name_length: u32,
    pub float_offset: u32,
    pub int_value: i32,
    pub texture_kind: u32,
    pub global_offset: u32,
    pub global_length: u32,
    pub resource: u64,
}

/// How many entries, floats and text bytes the bindings hold. On input they are the capacities
/// of the arrays a caller passes; on output they are what the bindings hold.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityComputeBindingsInfo {
    pub entry_count: u32,
    pub float_count: u32,
    pub text_length: u32,
}

/// The bindings laid out for a caller: entries, the floats they point into and the text.
#[derive(Default)]
struct Flat {
    entries: Vec<DisunityComputeBinding>,
    floats: Vec<f32>,
    text: Vec<u8>,
}

impl Flat {
    /// Appends a string to the text and returns its offset and length.
    fn string(&mut self, value: &str) -> Result<(u32, u32), AbiError> {
        let offset = u32::try_from(self.text.len()).map_err(|_| "text overflow")?;
        let length = u32::try_from(value.len()).map_err(|_| "text overflow")?;
        self.text.extend_from_slice(value.as_bytes());
        Ok((offset, length))
    }

    /// Appends an entry of `kind` named `name` whose floats are `floats`.
    fn push(
        &mut self,
        kind: u32,
        name: &str,
        floats: &[f32],
    ) -> Result<&mut DisunityComputeBinding, AbiError> {
        let (name_offset, name_length) = self.string(name)?;
        let float_offset = u32::try_from(self.floats.len()).map_err(|_| "float overflow")?;
        self.floats.extend_from_slice(floats);
        self.entries.push(DisunityComputeBinding {
            kind,
            name_offset,
            name_length,
            float_offset,
            ..Default::default()
        });
        Ok(self.entries.last_mut().expect("the entry was just pushed"))
    }

    fn texture(
        &mut self,
        name: &str,
        slot: &worldrunner_world_simulation::graphics::TextureSlot,
    ) -> Result<(), AbiError> {
        let (texture_kind, resource) = texture_parts(&slot.texture);
        let global = match &slot.texture {
            TextureRef::Global(global) => Some(self.string(global)?),
            _ => None,
        };
        let floats = [slot.scale[0], slot.scale[1], slot.offset[0], slot.offset[1]];
        let entry = self.push(KIND_TEXTURE, name, &floats)?;
        entry.texture_kind = texture_kind;
        entry.resource = resource;
        if let Some((offset, length)) = global {
            entry.global_offset = offset;
            entry.global_length = length;
        }
        Ok(())
    }

    fn of(bindings: &ComputeBindings) -> Result<Self, AbiError> {
        let properties = &bindings.properties;
        let mut flat = Self::default();
        for (name, value) in &properties.floats {
            flat.push(KIND_FLOAT, name, &[*value])?;
        }
        for (name, value) in &properties.ints {
            flat.push(KIND_INT, name, &[])?.int_value = *value;
        }
        for (name, value) in &properties.vectors {
            flat.push(KIND_VECTOR, name, value)?;
        }
        for (name, value) in &properties.matrices {
            flat.push(KIND_MATRIX, name, value)?;
        }
        for (name, slot) in &properties.textures {
            flat.texture(name, slot)?;
        }
        for (name, buffer) in &bindings.buffers {
            flat.push(KIND_BUFFER, name, &[])?.resource = buffer.to_bits();
        }
        Ok(flat)
    }
}

/// Reads the values of a dispatch record by its token. With null `entries` only `info` is filled
/// in, to size the arrays; otherwise the capacities in `info` must hold what the bindings have,
/// and the arrays are filled. `floats` and `text` may be null when the bindings have none of
/// them.
///
/// # Safety
/// `info` is readable and writable for one record; `entries`, `floats` and `text` are writable
/// for the capacities `info` states, or null as described.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_compute_bindings_read(
    world: u64,
    token: u32,
    info: *mut DisunityComputeBindingsInfo,
    entries: *mut DisunityComputeBinding,
    floats: *mut f32,
    text: *mut u8,
) -> DisunityResult {
    if info.is_null() {
        return DisunityResult::invalid_argument();
    }
    let capacity = unsafe { *info };
    mutate(world, |s| {
        let index = usize::try_from(token)
            .ok()
            .and_then(|token| token.checked_sub(1))
            .ok_or("a binding token is nonzero")?;
        let bindings = s
            .dispatch_bindings
            .get(index)
            .ok_or("the binding token names no dispatch of the last take")?;
        let flat = Flat::of(bindings)?;
        let count = |len: usize| u32::try_from(len).map_err(|_| AbiError::from("count overflow"));
        let held = DisunityComputeBindingsInfo {
            entry_count: count(flat.entries.len())?,
            float_count: count(flat.floats.len())?,
            text_length: count(flat.text.len())?,
        };
        if !entries.is_null() {
            if capacity.entry_count < held.entry_count
                || capacity.float_count < held.float_count
                || capacity.text_length < held.text_length
            {
                return Err("the binding capacities are too small".into());
            }
            if (floats.is_null() && held.float_count > 0)
                || (text.is_null() && held.text_length > 0)
            {
                return Err("the bindings hold floats or text, which need a place to go".into());
            }
            unsafe {
                std::ptr::copy_nonoverlapping(flat.entries.as_ptr(), entries, flat.entries.len());
                if !floats.is_null() {
                    std::ptr::copy_nonoverlapping(flat.floats.as_ptr(), floats, flat.floats.len());
                }
                if !text.is_null() {
                    std::ptr::copy_nonoverlapping(flat.text.as_ptr(), text, flat.text.len());
                }
            }
        }
        unsafe { *info = held };
        Ok(())
    })
}
