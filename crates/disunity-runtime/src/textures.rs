//! Texture bytes the managed engine loaded from a player's data, read from the same native image.
use crate::{DisunityHost, HostError, native};

/// One texture a player's managed code gave the renderer: the engine's `TextureFormat` value, the
/// size, the mip chain's length, the raw bytes (base level first) and the generation of this copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerTexture {
    pub format: u32,
    pub width: u32,
    pub height: u32,
    pub mips: u32,
    /// The texels are linear data rather than sRGB colour.
    pub linear: bool,
    /// Rises each time the managed engine stores the texture again.
    pub generation: u64,
    /// Zero for colour, data and coverage. For an `Alpha8` atlas holding signed distance fields,
    /// their spread in texels (see `DisunityTextureInfo`).
    pub distance_spread: u32,
    pub bytes: Vec<u8>,
}

/// How a texture is sampled. `wrap_*` are the engine's `TextureWrapMode` (0 repeat, 1 clamp,
/// 2 mirror, 3 mirror once) and `filter` its `FilterMode` (0 point, 1 bilinear, 2 trilinear).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextureSampler {
    /// Starts at 1 when the texture is first stored and rises each time a value changes.
    pub generation: u64,
    pub wrap_u: u32,
    pub wrap_v: u32,
    pub wrap_w: u32,
    pub filter: u32,
    /// 0 to 16.
    pub aniso_level: u32,
    pub mip_bias: f32,
}

pub(crate) type SamplerRead =
    unsafe extern "C" fn(u64, u64, *mut native::DisunitySampler) -> native::DisunityResult;

pub(crate) type Read = unsafe extern "C" fn(
    u64,
    u64,
    *mut native::DisunityTextureInfo,
    *mut u8,
    u32,
) -> native::DisunityResult;

impl DisunityHost {
    /// The texture stored under renderer `identity`, or `None` when the managed engine stored no
    /// such texture or its generation is still `known_generation`, so an unchanged texture costs no
    /// byte copy.
    pub fn player_texture(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerTexture>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        // The symbol belongs to the image this host retains.
        let read: Read = unsafe {
            *self
                ._library
                .get(b"disunity_texture_read\0")
                .map_err(|_| HostError::MissingSymbol("disunity_texture_read".into()))?
        };
        read_texture(world, read, identity, known_generation, |result| {
            self.check(result)
        })
    }

    /// The sampler of the texture stored under renderer `identity`, or `None` when the managed
    /// engine stored no such texture. Compare `generation` with the one held to see a change.
    pub fn texture_sampler(&self, identity: u64) -> Result<Option<TextureSampler>, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        // The symbol belongs to the image this host retains.
        let read: SamplerRead = unsafe {
            *self
                ._library
                .get(b"disunity_texture_sampler_read\0")
                .map_err(|_| HostError::MissingSymbol("disunity_texture_sampler_read".into()))?
        };
        read_sampler(world, read, identity, |result| self.check(result))
    }
}

/// Reads the texture stored under `identity` through `read`; see [`DisunityHost::player_texture`].
pub(crate) fn read_texture(
    world: u64,
    read: Read,
    identity: u64,
    known_generation: u64,
    check: impl Fn(native::DisunityResult) -> Result<(), HostError>,
) -> Result<Option<PlayerTexture>, HostError> {
    let mut info = native::DisunityTextureInfo::default();
    check(unsafe { read(world, identity, &mut info, std::ptr::null_mut(), 0) })?;
    if info.present == 0 || info.generation == known_generation {
        return Ok(None);
    }
    let mut bytes = vec![0; info.length as usize];
    check(unsafe { read(world, identity, &mut info, bytes.as_mut_ptr(), info.length) })?;
    Ok(Some(PlayerTexture {
        format: info.format,
        width: info.width,
        height: info.height,
        mips: info.mips,
        linear: info.linear != 0,
        generation: info.generation,
        distance_spread: info.distance_spread,
        bytes,
    }))
}

/// Reads the sampler of the texture stored under `identity` through `read`; see
/// [`DisunityHost::texture_sampler`].
pub(crate) fn read_sampler(
    world: u64,
    read: SamplerRead,
    identity: u64,
    check: impl Fn(native::DisunityResult) -> Result<(), HostError>,
) -> Result<Option<TextureSampler>, HostError> {
    let mut sampler = native::DisunitySampler::default();
    check(unsafe { read(world, identity, &mut sampler) })?;
    Ok((sampler.present != 0).then_some(TextureSampler {
        generation: sampler.generation,
        wrap_u: sampler.wrap_u,
        wrap_v: sampler.wrap_v,
        wrap_w: sampler.wrap_w,
        filter: sampler.filter,
        aniso_level: sampler.aniso_level,
        mip_bias: sampler.mip_bias,
    }))
}
