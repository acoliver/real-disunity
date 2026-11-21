//! The stock shadow cascades of the frame in flight, lent to a game's features to sample.
use super::lent::{FrameInputError, Lent};
use glam::{Mat4, Vec4};

/// How the stock receivers choose which cascade shades a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CascadeSelection {
    /// The first cascade whose light-space bounds contain the point. The split distances play no
    /// part in the choice.
    ByCoverage,
    /// The first cascade whose split distance is at or beyond the point's view depth, measured
    /// along [`FrameShadows::view_depth`].
    ByCameraDistance,
    /// Camera-distance cascades whose widths follow a recurrence: the first spans from
    /// [`FrameShadows::near`] to the first split, and each later one is
    /// [`FrameShadows::multiplier`] times as wide as the one before.
    Recurrence,
}

impl CascadeSelection {
    /// The value the stock receiver uniform carries for this rule.
    pub(crate) const fn uniform_word(self) -> f32 {
        match self {
            Self::ByCoverage => 0.0,
            Self::ByCameraDistance => 1.0,
            Self::Recurrence => 2.0,
        }
    }
}

/// One cascade of the stock shadow map, as the stock receivers read it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ShadowCascade {
    /// Clip-from-scene transform of the cascade's light. Clip depth is the value the casters
    /// wrote; x and y map to the layer's texture coordinates as `0.5 * x + 0.5` and
    /// `0.5 - 0.5 * y`.
    pub view_projection: Mat4,
    /// The far edge of the cascade in metres of view depth. It decides the choice of cascade
    /// only under [`CascadeSelection::ByCameraDistance`] and [`CascadeSelection::Recurrence`].
    pub split_distance: f32,
    /// The depth the stock receivers subtract from a point's light-space depth before they
    /// compare it with this cascade's layer.
    pub depth_bias: f32,
}

/// The stock shadow cascades the casters wrote for the frame in flight.
///
/// The views and the sampler belong to the frame: read them while the frame prepares and draws,
/// never write through them, and drop them with the frame. The next frame may write the same
/// texture again, and a replaced renderer frees it.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct FrameShadows {
    /// The cascades as a depth-only 2D array view, one layer per cascade, in the layout the stock
    /// receivers bind. A pass samples it with `texture_depth_2d_array` and
    /// [`Self::sampler`]. The casters of every cascade finish before any game pass begins.
    pub depth: wgpu::TextureView,
    /// The comparison sampler the stock receivers use with [`Self::depth`].
    pub sampler: wgpu::Sampler,
    /// Texels along one side of a cascade's layer.
    pub resolution: u32,
    /// The cascades in layer order. Their count is the number of layers in use.
    pub cascades: Vec<ShadowCascade>,
    /// How the stock receivers pick among the cascades.
    pub selection: CascadeSelection,
    /// The camera's near distance in metres, where the first camera-distance cascade begins.
    pub near: f32,
    /// A scene position `p` has view depth `view_depth.dot(p.extend(1.0))` metres.
    pub view_depth: Vec4,
    /// Fraction of a cascade's depth range, ending at its far edge, over which the receivers
    /// blend into the next cascade; zero when they do not blend.
    pub blend: f32,
    /// Width ratio between successive cascades under [`CascadeSelection::Recurrence`].
    pub multiplier: f32,
    /// Fraction of the shadow distance, ending at the last cascade, that fades to lit; zero when
    /// the shadows end abruptly.
    pub fade_out: f32,
    /// Distance in metres the stock receivers move a point along its normal before projecting it
    /// into a cascade.
    pub normal_bias_meters: f32,
    /// How much of the shadow the stock receivers apply: their direct light is multiplied by
    /// `mix(1, visibility, strength)`. The receiver uniform stores `1 - strength`.
    pub strength: f32,
}

/// Where a game reads the stock shadow cascades of the frame in flight. The shadowed route
/// publishes them once its cascades for the frame are final, before any feature prepares, so a
/// feature reads them in `prepare` and `draw` of the same frame. A route without shadows
/// publishes nothing.
///
/// A game pass that samples [`FrameShadows::depth`] declares no read of it. Every game place
/// whose feature may read the cascades sits after the shadow casters in the forward frame (a
/// depth-prefix feature reads the lent terrain only; shadow, occlusion and environment inputs
/// are `NotPrepared` there, see
/// [`GameFeatureFactory::build_depth_prefix`](super::GameFeatureFactory::build_depth_prefix)),
/// and the device orders the casters' writes before the pass's samples within the frame's
/// encoder.
/// [`GamePassSpec::reading`](super::GamePassSpec::reading) resolves names against the game's own
/// targets and refuses a stock resource.
#[derive(Clone, Debug, Default)]
pub struct FrameShadowHandle(Lent<FrameShadows>);

impl FrameShadowHandle {
    /// The shadow cascades of the frame now being encoded.
    ///
    /// # Errors
    /// [`FrameInputError::OutsideFrame`] when no frame is being prepared, and
    /// [`FrameInputError::NotPrepared`] when the frame in flight published none, including when
    /// the only publication belongs to an earlier frame.
    pub fn current(&self) -> Result<FrameShadows, FrameInputError> {
        self.0.current()
    }

    /// Stores `shadows` as the publication of the frame being prepared. A frame with no
    /// transaction in scope publishes nothing.
    pub(crate) fn publish(&self, shadows: FrameShadows) {
        self.0.publish(shadows);
    }
}
