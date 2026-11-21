//! The opaque snapshots of the frame in flight, lent to a game's features to sample.
//!
//! The frame copies its lit opaque colour and its opaque depth after the last Opaque entry and
//! the game's passes placed [`GamePlace::AfterOpaque`], and before the water and every other
//! transparent pass. A game pass placed at [`GamePlace::AfterOpaqueSnapshots`] samples the copies
//! through [`FrameSnapshotHandle`], which is how a forward pass draws refraction or depth fade
//! over the opaque scene without owning a copy of it.
//!
//! The frame also writes the depth after the water and the other transparent passes, the
//! post-water depth. A game pass placed at [`GamePlace::AfterTransparent`] samples it, which is how
//! a pass that follows the transparent passes finds the frame's final depth, including the depth
//! a game pass wrote at `AfterOpaqueSnapshots`, without owning a copy of it. The frame plans it
//! for such a pass even when the stock passes draw no water.
//!
//! [`GamePlace::AfterTransparent`]: worldrunner_render_post::GamePlace::AfterTransparent
//! [`GamePlace::AfterOpaque`]: worldrunner_render_post::GamePlace::AfterOpaque
//! [`GamePlace::AfterOpaqueSnapshots`]: worldrunner_render_post::GamePlace::AfterOpaqueSnapshots
use crate::compositor::FrameRenderer;
use crate::water::planning::SnapshotRequest;
use crate::water::snapshots::{
    COLOR, DEVICE_DEPTH, LINEAR_DEPTH, POST_WATER_DEPTH, POST_WATER_DEVICE_DEPTH,
};
use std::cell::RefCell;
use std::rc::Rc;
use worldrunner_render_gpu::submission::Publications;
use worldrunner_render_post::{FrameError, ResourceRef};

/// Which of the frame's opaque snapshots a game reads.
///
/// A factory states it in [`GameFeatureFactory::snapshot_reads`](super::GameFeatureFactory::snapshot_reads),
/// which is what lets the renderer reserve the snapshots in its allocation budget, allocate them
/// when it is built and admit them to a same-frame capture before any frame plans. A pass states
/// the ones it samples with [`GamePassSpec::reading_snapshots`](super::GamePassSpec::reading_snapshots),
/// which orders it after the snapshot writers and asks the frame to record them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotReads {
    /// The opaque scene colour, [`wgpu::TextureFormat::Rgba16Float`], at the frame's extent.
    /// Needs an HDR frame.
    pub color: bool,
    /// The opaque scene depth as positive view-space metres, [`wgpu::TextureFormat::R32Float`],
    /// at the frame's extent.
    pub linear_depth: bool,
    /// The opaque scene's device depth, [`wgpu::TextureFormat::R32Float`], at the frame's extent.
    /// It implies the linear depth, which the same pass writes.
    pub device_depth: bool,
    /// The frame's depth after the stock water and every other pass before the game's
    /// [`GamePlace::AfterTransparent`] passes, and which forms of it are read. Only a pass
    /// placed at `AfterTransparent` can read it. It is the frame's own depth buffer at that
    /// point, so it holds the depth a game pass wrote at `AfterOpaqueSnapshots` and the depth the
    /// stock water wrote when it draws.
    ///
    /// [`GamePlace::AfterTransparent`]: worldrunner_render_post::GamePlace::AfterTransparent
    pub post_water_depth: PostWaterDepth,
}

/// Which forms of the post-water depth a game reads. The stock product is linear metres only; the
/// device depth is a companion that the same pass writes, so it implies the linear depth.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PostWaterDepth {
    /// Neither form.
    #[default]
    Unread,
    /// Positive view-space metres, [`wgpu::TextureFormat::R32Float`], at the frame's extent.
    Linear,
    /// The metres and the depth buffer's own values, [`wgpu::TextureFormat::R32Float`], at the
    /// frame's extent.
    LinearAndDevice,
}

impl PostWaterDepth {
    /// The resources a pass declares to read these forms.
    fn resources(self) -> Vec<&'static str> {
        match self {
            Self::Unread => Vec::new(),
            Self::Linear => vec![POST_WATER_DEPTH],
            Self::LinearAndDevice => vec![POST_WATER_DEPTH, POST_WATER_DEVICE_DEPTH],
        }
    }
}

impl SnapshotReads {
    pub const NONE: Self = Self {
        color: false,
        linear_depth: false,
        device_depth: false,
        post_water_depth: PostWaterDepth::Unread,
    };
    pub const ALL: Self = Self {
        color: true,
        linear_depth: true,
        device_depth: true,
        post_water_depth: PostWaterDepth::LinearAndDevice,
    };

    #[must_use]
    pub const fn any(self) -> bool {
        self.color
            || self.linear_depth
            || self.device_depth
            || !matches!(self.post_water_depth, PostWaterDepth::Unread)
    }

    /// Whether every snapshot `other` reads is read here too.
    #[must_use]
    pub const fn covers(self, other: Self) -> bool {
        (self.color || !other.color)
            && (self.linear_depth || !other.linear_depth)
            && (self.device_depth || !other.device_depth)
            && self.post_water_depth as u8 >= other.post_water_depth as u8
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            color: self.color || other.color,
            linear_depth: self.linear_depth || other.linear_depth,
            device_depth: self.device_depth || other.device_depth,
            post_water_depth: if self.post_water_depth as u8 >= other.post_water_depth as u8 {
                self.post_water_depth
            } else {
                other.post_water_depth
            },
        }
    }

    /// The resources a pass declares to read these snapshots.
    pub(crate) fn resources(self) -> Vec<ResourceRef> {
        [
            (self.color, COLOR),
            (self.linear_depth, LINEAR_DEPTH),
            (self.device_depth, DEVICE_DEPTH),
        ]
        .into_iter()
        .filter(|(read, _)| *read)
        .map(|(_, name)| name)
        .chain(self.post_water_depth.resources())
        .map(ResourceRef::new)
        .collect()
    }
}

/// One snapshot texture of the frame in flight.
///
/// The texture belongs to the frame's slot. Sample it while the frame prepares and draws, never
/// write through it, and drop it with the frame: the slot's next frame writes it again.
#[derive(Clone, Debug)]
pub struct SnapshotImage {
    pub texture: wgpu::Texture,
    /// The whole texture as a 2D view. Both formats sample as unfilterable float.
    pub view: wgpu::TextureView,
}

impl SnapshotImage {
    pub(crate) fn of(texture: &wgpu::Texture) -> Self {
        Self {
            view: texture.create_view(&wgpu::TextureViewDescriptor::default()),
            texture: texture.clone(),
        }
    }
}

/// The opaque snapshots the frame in flight wrote.
///
/// A field is present when the frame wrote that snapshot, which it does whenever a pass of the
/// frame declared a read of it (the stock water and reflections declare some too), so a pass that
/// declared its reads finds them all here. Every snapshot is the frame's extent, taken after the
/// last Opaque entry and every game pass placed [`AfterOpaque`](worldrunner_render_post::GamePlace::AfterOpaque).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct FrameSnapshots {
    /// The lit opaque colour. Transparent passes, including the water, have not drawn into it.
    /// Samples of a multisampled frame are averaged.
    pub color: Option<SnapshotImage>,
    /// Positive view-space metres along the camera's forward axis, reconstructed from the nearest
    /// sample of each pixel. A pixel no opaque surface covers holds the distance of the far plane.
    pub linear_depth: Option<SnapshotImage>,
    /// The depth buffer's own values. The engine's depth is not reversed: `0.0` is the near plane,
    /// `1.0` is the far plane and the cleared value, and a surface is nearer when its value is
    /// smaller. A multisampled frame keeps the smallest sample, the nearest, of each pixel.
    pub device_depth: Option<SnapshotImage>,
    /// Positive view-space metres of the frame's depth after the stock water and every pass
    /// before [`AfterTransparent`](worldrunner_render_post::GamePlace::AfterTransparent), from the
    /// nearest sample of each pixel. It includes the depth a game pass wrote at
    /// `AfterOpaqueSnapshots` and the depth the stock water wrote, if it draws. Only a pass at
    /// `AfterTransparent` can read it.
    pub post_water_depth: Option<SnapshotImage>,
    /// The depth buffer's own values at the same point, in the convention of
    /// [`device_depth`](Self::device_depth).
    pub post_water_device_depth: Option<SnapshotImage>,
}

impl FrameSnapshots {
    /// Whether the frame wrote any snapshot.
    pub(crate) const fn any(&self) -> bool {
        self.color.is_some()
            || self.linear_depth.is_some()
            || self.device_depth.is_some()
            || self.post_water_depth.is_some()
            || self.post_water_device_depth.is_some()
    }
}

/// Why a frame cannot lend its snapshots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameSnapshotError {
    /// No frame is being prepared, so no snapshots are current. The snapshots of an earlier frame
    /// are never returned.
    #[error("opaque snapshots are lent only while a frame is being encoded")]
    OutsideFrame,
    /// The frame in flight wrote none of the snapshots: no pass of it declared a read of one.
    #[error("the frame in flight wrote no opaque snapshots")]
    NotPrepared,
}

impl From<FrameSnapshotError> for FrameError {
    fn from(error: FrameSnapshotError) -> Self {
        Self::FeaturePreparation {
            feature: "frame snapshots",
            reason: error.to_string(),
        }
    }
}

/// What a frame published, with the frame it belongs to.
#[derive(Debug)]
struct Published {
    frame: Publications,
    snapshots: FrameSnapshots,
}

/// Where a game reads the opaque snapshots of the frame in flight. The renderer publishes them as
/// soon as the frame is planned, before any feature prepares, so a feature reads them in
/// `prepare` and `draw` of the same frame.
///
/// A pass samples a snapshot only after declaring the read with
/// [`GamePassSpec::reading_snapshots`](super::GamePassSpec::reading_snapshots): the declaration
/// is what orders the pass after the snapshot writers. Placing the pass at
/// [`GamePlace::AfterOpaqueSnapshots`](worldrunner_render_post::GamePlace::AfterOpaqueSnapshots)
/// keeps it ahead of the transparent passes.
#[derive(Clone, Debug, Default)]
pub struct FrameSnapshotHandle(Rc<RefCell<Option<Published>>>);

impl FrameSnapshotHandle {
    /// The snapshots of the frame now being encoded.
    ///
    /// # Errors
    /// [`FrameSnapshotError::OutsideFrame`] when no frame is being prepared, and
    /// [`FrameSnapshotError::NotPrepared`] when the frame in flight wrote none, including when
    /// the only publication belongs to an earlier frame.
    pub fn current(&self) -> Result<FrameSnapshots, FrameSnapshotError> {
        let frame = Publications::current().ok_or(FrameSnapshotError::OutsideFrame)?;
        let mut slot = self.0.borrow_mut();
        if let Some(published) = slot.as_ref() {
            if published.frame.same_frame(&frame) {
                return Ok(published.snapshots.clone());
            }
            // An earlier frame's textures are of no use to anyone; do not keep them alive.
            *slot = None;
        }
        Err(FrameSnapshotError::NotPrepared)
    }

    /// Stores `snapshots` as the publication of the frame being prepared. A frame with no
    /// transaction in scope publishes nothing.
    pub(crate) fn publish(&self, snapshots: FrameSnapshots) {
        if let Some(frame) = Publications::current() {
            *self.0.borrow_mut() = Some(Published { frame, snapshots });
        }
    }
}

/// How much snapshot storage a renderer holds, for tests and diagnostics of what a configuration
/// costs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SnapshotStatistics {
    /// Frame slots that own the opaque colour and depth snapshots.
    pub slots: usize,
    /// Frame slots that also own the device depth snapshot, which only a game that reads it adds.
    pub device_depth_slots: usize,
    /// Frame slots that also own the post-water device depth, which only a game that reads it
    /// adds.
    pub post_water_device_depth_slots: usize,
}

impl FrameRenderer {
    /// Allocates the snapshots the game's factories declared they read, in every frame slot, and
    /// gives the slots the device depth copies when a factory reads them. It does nothing for a
    /// renderer whose game reads none, so such a renderer allocates no snapshot of its own.
    pub(crate) fn ensure_game_snapshots(&mut self, device: &wgpu::Device) {
        let reads = self.game.snapshot_reads;
        if !reads.any() {
            return;
        }
        let config = self.targets().config();
        let slots = self.snapshots.get_or_insert_with(|| {
            std::array::from_fn(|_| crate::water::snapshots::Snapshots::new(device, config))
        });
        let post_water_device = reads.post_water_depth == PostWaterDepth::LinearAndDevice;
        if reads.device_depth || post_water_device {
            for slot in slots.iter_mut() {
                *slot = slot.with_device_depths(device, reads.device_depth, post_water_device);
            }
        }
    }

    /// Lends the snapshots the frame's schedule `request`s to the game's features. A renderer with
    /// no game features, or a frame that writes none of the snapshots a game can read, publishes
    /// nothing.
    pub(crate) fn publish_game_snapshots(&self, request: SnapshotRequest) {
        let Some(slots) = self.snapshots.as_ref().filter(|_| self.game.features > 0) else {
            return;
        };
        let bank = &slots[self.frame_slot];
        let lent = FrameSnapshots {
            color: request.color.then(|| SnapshotImage::of(&bank.color)),
            linear_depth: request.depth.then(|| SnapshotImage::of(&bank.depth)),
            device_depth: bank
                .device_depth
                .as_ref()
                .filter(|_| request.device.opaque)
                .map(|depth| SnapshotImage::of(&depth.texture)),
            post_water_depth: request
                .final_depth
                .then(|| SnapshotImage::of(&bank.final_depth)),
            post_water_device_depth: bank
                .final_device_depth
                .as_ref()
                .filter(|_| request.device.post_water)
                .map(|depth| SnapshotImage::of(&depth.texture)),
        };
        if lent.any() {
            self.game.snapshots.publish(lent);
        }
    }

    /// What snapshot storage this renderer holds now.
    pub(crate) fn snapshot_statistics(&self) -> SnapshotStatistics {
        self.snapshots
            .as_ref()
            .map_or_else(SnapshotStatistics::default, |slots| SnapshotStatistics {
                slots: slots.len(),
                device_depth_slots: slots
                    .iter()
                    .filter(|slot| slot.device_depth.is_some())
                    .count(),
                post_water_device_depth_slots: slots
                    .iter()
                    .filter(|slot| slot.final_device_depth.is_some())
                    .count(),
            })
    }
}
