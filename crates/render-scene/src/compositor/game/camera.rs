//! The frame camera a game's passes draw with: the minimum the stock passes share.
use glam::{Mat4, Vec3};
use std::cell::RefCell;
use std::rc::Rc;
use worldrunner_render_post::FrameError;

/// The camera of the frame being encoded, as every stock pass sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameCamera {
    /// Clip-from-scene transform the stock passes rasterise with. It carries the sub-pixel jitter
    /// while projection jitter is on.
    pub view_projection: Mat4,
    /// The same transform without jitter, which CPU selection and temporal reprojection use.
    pub unjittered_view_projection: Mat4,
    /// Right-handed view transform for reconstructing positions from depth.
    pub view: Mat4,
    /// Eye position in scene space, metres, Z-up.
    pub camera_position: Vec3,
    /// This frame's sub-pixel offset in raster pixels, +y down; `None` while jitter is off.
    pub jitter_pixels: Option<[f32; 2]>,
    /// The unjittered transform of the previous published frame; `None` while jitter is off.
    pub previous_view_projection: Option<Mat4>,
    /// The raster extent in pixels.
    pub raster_viewport: [u32; 2],
    /// The winding the frame's camera convention counts as a front face.
    pub front_face: wgpu::FrontFace,
    /// The admitted frame slot being encoded.
    pub frame_slot: usize,
}

impl FrameCamera {
    pub(crate) fn of(frame: &crate::frame::FrameUniform, frame_slot: usize) -> Self {
        Self {
            view_projection: frame.view_projection,
            unjittered_view_projection: frame.selection_view_projection(),
            view: frame.view,
            camera_position: frame.camera_position,
            jitter_pixels: frame.temporal.map(|temporal| temporal.jitter_pixels),
            previous_view_projection: frame
                .temporal
                .map(|temporal| temporal.previous_view_projection),
            raster_viewport: frame.raster_viewport,
            front_face: frame.front_face(),
            frame_slot,
        }
    }
}

/// Where a game reads the camera of the frame in flight. The renderer sets it as it plans each
/// frame, before any feature prepares, so a feature reads it in `prepare` and `plan`.
#[derive(Clone, Debug, Default)]
pub struct FrameCameraHandle(Rc<RefCell<Option<FrameCamera>>>);

impl FrameCameraHandle {
    /// The camera of the frame now being encoded.
    ///
    /// # Errors
    /// [`FrameError::FeaturePreparation`] outside a frame, before the first one is planned.
    pub fn current(&self) -> Result<FrameCamera, FrameError> {
        (*self.0.borrow()).ok_or_else(|| FrameError::FeaturePreparation {
            feature: "frame camera",
            reason: "no frame has been planned yet".to_owned(),
        })
    }

    pub(crate) fn set(&self, camera: &FrameCamera) {
        *self.0.borrow_mut() = Some(*camera);
    }
}
