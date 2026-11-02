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

type Read = unsafe extern "C" fn(
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
        let mut info = native::DisunityTextureInfo::default();
        self.check(unsafe { read(world, identity, &mut info, std::ptr::null_mut(), 0) })?;
        if info.present == 0 || info.generation == known_generation {
            return Ok(None);
        }
        let mut bytes = vec![0; info.length as usize];
        self.check(unsafe { read(world, identity, &mut info, bytes.as_mut_ptr(), info.length) })?;
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
}
