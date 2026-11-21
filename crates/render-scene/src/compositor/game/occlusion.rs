//! The stock screen-space ambient occlusion a game's passes sample.
use super::lent::{FrameInputError, Lent};
use worldrunner_render_post::ResourceRef;

/// The filtered ambient occlusion of the frame in flight, the texture the stock receivers sample.
///
/// The red channel is visibility in `[0, 1]`: 1 where nothing occludes the pixel, lower where
/// nearby geometry does; the stock receivers multiply their ambient light by it.
/// [`OcclusionDepth`] says what the green channel holds. The texture covers the whole frame, at
/// a size smaller than the frame's raster for kernels that work on a reduced depth footprint:
/// sample it at the frame's normalised screen position.
#[derive(Clone, Debug)]
pub struct FrameOcclusion {
    /// The occlusion: `R32Float`, `Rg16Float` or `Rg32Float`. The 32-bit formats are not
    /// filterable without the device's float filtering feature, so gather or load texels.
    pub texture: wgpu::Texture,
    /// A two-dimensional view of the whole texture.
    pub view: wgpu::TextureView,
    /// What the green channel holds.
    pub depth: OcclusionDepth,
    /// Whether the stock receivers sample it. When they do not, the frame computes it only
    /// because a pass declares [`FrameOcclusion::read`].
    pub stock_receivers: bool,
}

impl FrameOcclusion {
    /// The frame resource a pass that samples [`Self::view`] declares with
    /// [`GamePassSpec::sampling_frame`](super::GamePassSpec::sampling_frame). The declaration also
    /// makes the frame compute the occlusion when no stock receiver samples it.
    pub fn read() -> ResourceRef {
        crate::ssao::passes::occlusion_resource()
    }
}

/// What the green channel of a [`FrameOcclusion`] holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OcclusionDepth {
    /// Nothing: the texture has one channel.
    None,
    /// The linear view depth of the texel the occlusion was computed at, in metres in front of
    /// the camera, for receivers that weigh neighbouring texels by depth.
    Meters,
    /// The same linear view depth divided by the distance of the camera's far plane, in `[0, 1]`.
    FractionOfFar,
}

/// Where a game reads the stock ambient occlusion of the frame in flight.
///
/// Every route publishes it as it begins encoding the frame, before any feature plans. The
/// texture is the renderer's own: sample it in the frame that lent it and never write it.
#[derive(Clone, Debug, Default)]
pub struct FrameOcclusionHandle(Lent<Option<FrameOcclusion>>);

impl FrameOcclusionHandle {
    /// The occlusion of the frame now being prepared; `None` when the renderer computes no
    /// screen-space ambient occlusion.
    ///
    /// # Errors
    /// [`FrameInputError::OutsideFrame`] outside a frame, and [`FrameInputError::NotPrepared`]
    /// before the frame has begun encoding.
    pub fn current(&self) -> Result<Option<FrameOcclusion>, FrameInputError> {
        self.0.current()
    }

    pub(crate) fn publish(&self, occlusion: Option<FrameOcclusion>) {
        self.0.publish(occlusion);
    }
}
