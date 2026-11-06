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

/// How a texture is sampled. `wrap_*` are the engine's `TextureWrapMode` (0 repeat, 1 clamp,
/// 2 mirror, 3 mirror once) and `filter` its `FilterMode` (0 point, 1 bilinear, 2 trilinear);
/// `aniso_level` is 0 to 16. On a write, `generation`, `present` and `changed` are ignored. On a
/// read, `generation` is the generation the caller already holds; the call fills in everything,
/// `present` is zero when the identity has no stored texture, and `changed` is nonzero when the
/// stored sampler is newer than the generation passed.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DisunitySampler {
    /// Starts at 1 when the texture is first stored and rises each time a write changes a value.
    pub generation: u64,
    pub wrap_u: u32,
    pub wrap_v: u32,
    pub wrap_w: u32,
    pub filter: u32,
    pub aniso_level: u32,
    pub present: u32,
    pub changed: u32,
    pub mip_bias: f32,
}

/// The engine's choice of sampler for a texture nothing has configured: repeat, bilinear, no
/// anisotropy beyond one and no bias.
const DEFAULT_SAMPLER: DisunitySampler = DisunitySampler {
    generation: 1,
    wrap_u: 0,
    wrap_v: 0,
    wrap_w: 0,
    filter: 1,
    aniso_level: 1,
    present: 1,
    changed: 0,
    mip_bias: 0.0,
};

/// The highest wrap mode value.
const MAX_WRAP: u32 = 3;
/// The highest filter mode value.
const MAX_FILTER: u32 = 2;
/// The highest anisotropy level.
const MAX_ANISO: u32 = 16;

/// One texture of a world's session, kept until the world is unloaded.
pub(crate) struct StoredTexture {
    info: DisunityTextureInfo,
    bytes: Vec<u8>,
    sampler: DisunitySampler,
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
        let held = s.textures.get(&identity);
        let generation = held.map_or(1, |held| held.info.generation + 1);
        let sampler = held.map_or(DEFAULT_SAMPLER, |held| held.sampler);
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
                sampler,
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

fn check_sampler(sampler: &DisunitySampler) -> Result<(), &'static str> {
    let wraps = [sampler.wrap_u, sampler.wrap_v, sampler.wrap_w];
    if wraps.iter().any(|wrap| *wrap > MAX_WRAP) {
        return Err("a texture wrap mode is not one of the engine's four");
    }
    if sampler.filter > MAX_FILTER {
        return Err("a texture filter mode is not one of the engine's three");
    }
    if sampler.aniso_level > MAX_ANISO {
        return Err("a texture anisotropy level is above 16");
    }
    if !sampler.mip_bias.is_finite() {
        return Err("a texture mip bias is not finite");
    }
    Ok(())
}

/// Sets the sampler of a stored texture, raising its generation when a value changes.
///
/// # Safety
/// `sampler` is readable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_texture_sampler_write(
    world: u64,
    identity: u64,
    sampler: *const DisunitySampler,
) -> DisunityResult {
    if sampler.is_null() {
        return DisunityResult::invalid_argument();
    }
    let sampler = unsafe { *sampler };
    mutate(world, |s| {
        check_sampler(&sampler)?;
        let held = s
            .textures
            .get_mut(&identity)
            .ok_or("a sampler needs a texture stored under the same identity")?;
        let next = DisunitySampler {
            generation: held.sampler.generation,
            present: 1,
            changed: 0,
            ..sampler
        };
        if next != held.sampler {
            held.sampler = DisunitySampler {
                generation: held.sampler.generation + 1,
                ..next
            };
        }
        Ok(())
    })
}

/// Reports the sampler of a stored texture. An identity nothing stored reports `present` zero
/// and is not an error.
///
/// # Safety
/// `out` is readable and writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_texture_sampler_read(
    world: u64,
    identity: u64,
    out: *mut DisunitySampler,
) -> DisunityResult {
    if out.is_null() {
        return DisunityResult::invalid_argument();
    }
    let known = unsafe { (*out).generation };
    mutate(world, |s| {
        let sampler = s.textures.get(&identity).map(|held| DisunitySampler {
            changed: u32::from(held.sampler.generation > known),
            ..held.sampler
        });
        unsafe { *out = sampler.unwrap_or_default() };
        Ok(())
    })
}
