//! Texture bytes the managed engine hands to the host for textures it loaded from a player's
//! data. The renderer cannot see managed memory, so the host reads them from here.
use std::collections::BTreeMap;

use crate::{DisunityResult, callbacks::mutate};

/// Player textures take renderer identities from here up; lower identities belong to the
/// presentation (glyph coverage) and to textures the host decodes itself.
pub const FIRST_PLAYER_TEXTURE: u64 = 1 << 40;

/// The largest texture payload accepted, 512 MiB: an 8192 by 8192 RGBA32 image with room to spare.
const MAX_TEXTURE_BYTES: u32 = 1 << 29;

/// A stored texture's description. `format` is the engine's `TextureFormat` value. `present` is zero when a read names an identity nothing stored. `distance_spread` is the spread in texels of the signed distance fields in an `Alpha8` atlas (0.5 at a glyph's edge, `1 / (2 * spread)` per texel), or zero.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisunityTextureInfo {
    pub identity: u64,
    /// Starts at 1 and rises by one each time the identity is stored again.
    pub generation: u64,
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    /// Nonzero when the texels are linear data rather than sRGB colour.
    pub linear: u32,
    pub length: u32,
    pub present: u32,
    pub distance_spread: u32,
}

/// One texture of a world's session, kept until the world is unloaded.
pub(crate) struct StoredTexture {
    info: DisunityTextureInfo,
    bytes: Vec<u8>,
}

/// The textures of one session, by renderer identity.
pub(crate) type TextureStore = BTreeMap<u64, StoredTexture>;

fn check_info(identity: u64, info: &DisunityTextureInfo, length: u32) -> Result<(), &'static str> {
    if identity < FIRST_PLAYER_TEXTURE {
        return Err("a stored texture needs a player identity of 2^40 or more");
    }
    if info.identity != identity || info.length != length {
        return Err("the texture description disagrees with the identity or length passed");
    }
    if info.width == 0 || info.height == 0 || info.mips == 0 {
        return Err("a stored texture needs a nonzero size and mip count");
    }
    if length > MAX_TEXTURE_BYTES {
        return Err("the texture payload is larger than the host accepts");
    }
    Ok(())
}

/// Keeps a texture's bytes for the host and raises its generation.
///
/// # Safety
/// `info` is readable for one record; `bytes` is readable for `length` bytes, or null when
/// `length` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_texture_store(
    world: u64,
    identity: u64,
    info: *const DisunityTextureInfo,
    bytes: *const u8,
    length: u32,
) -> DisunityResult {
    if info.is_null() || (length > 0 && bytes.is_null()) {
        return DisunityResult::invalid_argument();
    }
    let info = unsafe { *info };
    mutate(world, |s| {
        check_info(identity, &info, length)?;
        let payload = if length == 0 {
            Vec::new()
        } else {
            unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec()
        };
        let generation = s
            .textures
            .get(&identity)
            .map_or(1, |held| held.info.generation + 1);
        let info = DisunityTextureInfo {
            generation,
            present: 1,
            ..info
        };
        s.textures.insert(
            identity,
            StoredTexture {
                info,
                bytes: payload,
            },
        );
        Ok(())
    })
}

/// Reports a stored texture and copies its bytes when `capacity` holds them. A zero capacity
/// copies nothing. An identity nothing stored reports `present` zero and is not an error.
///
/// # Safety
/// `out` is writable for one record; `bytes` is writable for `capacity` bytes, or null when
/// `capacity` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_texture_read(
    world: u64,
    identity: u64,
    out: *mut DisunityTextureInfo,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    if out.is_null() || (capacity != 0 && bytes.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let Some(held) = s.textures.get(&identity) else {
            unsafe {
                *out = DisunityTextureInfo {
                    identity,
                    ..Default::default()
                };
            }
            return Ok(());
        };
        unsafe { *out = held.info };
        if capacity == 0 {
            return Ok(());
        }
        if capacity < held.info.length {
            return Err("texture copy capacity is too small".into());
        }
        unsafe { std::ptr::copy_nonoverlapping(held.bytes.as_ptr(), bytes, held.bytes.len()) };
        Ok(())
    })
}
