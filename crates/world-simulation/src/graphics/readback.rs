//! Reading pixels back from a render texture.
//!
//! A script asks for a rectangle of a render texture and the world records a
//! [`OffscreenWork::ReadPixels`] at that place in the offscreen list. The host draws the work
//! before it, reads the rectangle and writes the texels with [`World::write_texture_pixels`]; the
//! script takes them with [`World::take_texture_pixels`] after the flush. A texel is four floats
//! (red, green, blue, alpha) as the render texture stores them, so an sRGB texture is not
//! decoded. Rows come bottom first: row 0 is the bottom row of the texture.
use super::{GraphicsError, OffscreenWork, RenderTextureId};
use crate::{LimitKind, World, WorldError};
use std::collections::BTreeMap;

/// Most requests a world holds open at once, answered or not.
pub const MAX_PENDING_READBACKS: usize = 1024;
/// Most texels one request reads.
pub const MAX_READBACK_TEXELS: u64 = 1 << 26;
/// Floats one texel holds.
const FLOATS_PER_TEXEL: u64 = 4;

/// A request and, once the host answered, its texels.
#[cfg_attr(test, derive(Clone))]
struct Readback {
    /// Floats the answer must hold.
    floats: usize,
    answer: Option<Vec<f32>>,
}

/// The requests of a world that nobody took, by id, and the last id issued.
#[cfg_attr(test, derive(Clone))]
pub(super) struct Readbacks {
    open: BTreeMap<u64, Readback>,
    last_id: u64,
}

impl Readbacks {
    pub(super) const fn new() -> Self {
        Self {
            open: BTreeMap::new(),
            last_id: 0,
        }
    }

    /// Forgets every open request; ids keep increasing.
    pub(super) fn clear(&mut self) {
        self.open.clear();
    }
}

/// A rectangle of texels, bottom row first.
#[derive(Clone, Copy)]
pub(super) struct Rect {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) width: u32,
    pub(super) height: u32,
}

impl World {
    /// Records a read of the `width` by `height` texels of `texture` whose bottom left texel is
    /// at (`x`, `y`) and returns the id of the request. The rectangle must be non-empty, lie
    /// inside the texture and hold at most [`MAX_READBACK_TEXELS`] texels, and a world holds at
    /// most [`MAX_PENDING_READBACKS`] open requests. The frame's end forgets requests nobody took.
    pub fn request_texture_pixels(
        &mut self,
        texture: RenderTextureId,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) -> Result<u64, WorldError> {
        let rect = Rect {
            x,
            y,
            width,
            height,
        };
        let floats = self.check_read_rect(texture, rect)?;
        if self.graphics.readbacks.open.len() >= MAX_PENDING_READBACKS {
            return Err(WorldError::CapacityExceeded(LimitKind::Graphics));
        }
        let request = self
            .graphics
            .readbacks
            .last_id
            .checked_add(1)
            .ok_or(WorldError::CapacityExceeded(LimitKind::Graphics))?;
        self.push_offscreen(OffscreenWork::ReadPixels {
            request,
            texture,
            x,
            y,
            width,
            height,
        })?;
        self.graphics.readbacks.last_id = request;
        self.graphics.readbacks.open.insert(
            request,
            Readback {
                floats,
                answer: None,
            },
        );
        Ok(request)
    }

    /// Stores the texels the host read for a request: `width * height * 4` floats, bottom row
    /// first. A second write replaces the first. Fails for an id no open request has, and for a
    /// wrong count, which leaves the request open.
    pub fn write_texture_pixels(&mut self, request: u64, rgba: &[f32]) -> Result<(), WorldError> {
        let held = self
            .graphics
            .readbacks
            .open
            .get_mut(&request)
            .ok_or(GraphicsError::UnknownReadback)?;
        if rgba.len() != held.floats {
            return Err(GraphicsError::ReadbackLength.into());
        }
        held.answer = Some(rgba.to_vec());
        Ok(())
    }

    /// Takes the texels the host wrote for a request, which closes the request. `None` when the
    /// host wrote nothing or the id names no open request.
    pub fn take_texture_pixels(&mut self, request: u64) -> Option<Vec<f32>> {
        self.graphics
            .readbacks
            .open
            .remove(&request)
            .and_then(|held| held.answer)
    }

    /// Checks a read rectangle and returns the number of floats it holds.
    pub(super) fn check_read_rect(
        &self,
        texture: RenderTextureId,
        rect: Rect,
    ) -> Result<usize, WorldError> {
        let held = self.render_texture(texture)?;
        let inside = |origin: u32, size: u32, limit: u32| {
            size > 0 && origin.checked_add(size).is_some_and(|end| end <= limit)
        };
        let texels = u64::from(rect.width) * u64::from(rect.height);
        if !inside(rect.x, rect.width, held.width)
            || !inside(rect.y, rect.height, held.height)
            || texels > MAX_READBACK_TEXELS
        {
            return Err(GraphicsError::ReadbackRect.into());
        }
        usize::try_from(texels * FLOATS_PER_TEXEL).map_err(|_| GraphicsError::ReadbackRect.into())
    }
}
