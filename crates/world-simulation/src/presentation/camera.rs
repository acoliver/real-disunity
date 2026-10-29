//! A camera component's settings and the rules for what a frame can present of them.
use super::{invalid, lighting};
use crate::WorldError;

/// What a camera draws before its content, with Unity's `CameraClearFlags` values.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum CameraClear {
    /// The world has no skybox material, so Unity clears to the background colour.
    Skybox = 1,
    #[default]
    SolidColor = 2,
    /// Keeps a lower camera's colour, which a single-camera frame does not have.
    DepthOnly = 3,
    /// Keeps a lower camera's colour and depth, which a single-camera frame does not have.
    Nothing = 4,
}
impl CameraClear {
    pub fn from_flags(flags: u32) -> Option<Self> {
        [
            Self::Skybox,
            Self::SolidColor,
            Self::DepthOnly,
            Self::Nothing,
        ]
        .into_iter()
        .find(|clear| *clear as u32 == flags)
    }
}

/// Which lens a camera uses. Both sizes stay stored, as in Unity, so switching back restores them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u32)]
pub enum Projection {
    #[default]
    Orthographic = 0,
    Perspective = 1,
}
impl Projection {
    pub fn from_value(value: u32) -> Option<Self> {
        [Self::Orthographic, Self::Perspective]
            .into_iter()
            .find(|projection| *projection as u32 == value)
    }
}

/// On/off camera settings that do not change which frame a 2D world draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraFlags(u32);
impl CameraFlags {
    pub const HDR: Self = Self(1);
    pub const MSAA: Self = Self(2);
    pub const DYNAMIC_RESOLUTION: Self = Self(4);
    pub const OCCLUSION_CULLING: Self = Self(8);
    const ALL: u32 = 15;

    /// Unity's defaults: HDR, MSAA and occlusion culling on, dynamic resolution off.
    pub const DEFAULT: Self = Self(Self::HDR.0 | Self::MSAA.0 | Self::OCCLUSION_CULLING.0);

    pub fn from_bits(bits: u32) -> Option<Self> {
        (bits & !Self::ALL == 0).then_some(Self(bits))
    }
    pub fn bits(self) -> u32 {
        self.0
    }
    pub fn contains(self, flag: Self) -> bool {
        self.0 & flag.0 == flag.0
    }
    #[must_use]
    pub fn with(self, flag: Self, on: bool) -> Self {
        Self(if on {
            self.0 | flag.0
        } else {
            self.0 & !flag.0
        })
    }
}
impl Default for CameraFlags {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Unity's `CameraType.Game`.
pub const CAMERA_TYPE_GAME: u32 = 1;
const CAMERA_TYPE_BITS: u32 = 0x1f;
/// Unity's `DepthTextureMode` flags: `Depth`, `DepthNormals` and `MotionVectors`.
const DEPTH_TEXTURE_BITS: u32 = 0b111;
/// Unity supports eight displays.
pub const DISPLAY_COUNT: u32 = 8;

/// A camera. Frames present orthographic cameras that draw the full screen; the other settings are
/// stored for scripts and rejected by [`Camera::clear_color`] when they would change the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    pub enabled: bool,
    pub projection: Projection,
    /// Half the visible height of an orthographic camera, in world units.
    pub half_height: f32,
    /// Vertical field of view of a perspective camera, in degrees.
    pub field_of_view: f32,
    pub near: f32,
    pub far: f32,
    pub clear: CameraClear,
    pub background: [f64; 4],
    pub depth: i32,
    /// One bit per `GameObject` layer; objects on layers outside the mask are not drawn.
    pub culling_mask: u32,
    /// Normalized screen rectangle `[x, y, width, height]` the camera draws into.
    pub viewport: [f32; 4],
    pub depth_texture_mode: u32,
    pub flags: CameraFlags,
    pub target_display: u32,
    /// Instance id of the managed `RenderTexture` the camera draws into; zero draws to the screen.
    pub target_texture: u64,
    /// Replaces the projection derived from the lens settings, as column-major floats.
    pub projection_matrix: Option<[f32; 16]>,
    /// Replaces the view derived from the object's transform, as column-major floats.
    pub world_to_camera: Option<[f32; 16]>,
    /// Unity's `CameraType` flags.
    pub camera_type: u32,
}
impl Default for Camera {
    fn default() -> Self {
        Self {
            enabled: true,
            projection: Projection::Orthographic,
            half_height: 5.,
            field_of_view: 60.,
            near: 0.01,
            far: 1000.,
            clear: CameraClear::SolidColor,
            background: [0., 0., 0., 1.],
            depth: 0,
            culling_mask: u32::MAX,
            viewport: [0., 0., 1., 1.],
            depth_texture_mode: 0,
            flags: CameraFlags::DEFAULT,
            target_display: 0,
            target_texture: 0,
            projection_matrix: None,
            world_to_camera: None,
            camera_type: CAMERA_TYPE_GAME,
        }
    }
}
impl Camera {
    pub fn validate(self) -> Result<(), WorldError> {
        let lens_valid = [self.half_height, self.near, self.far]
            .into_iter()
            .all(f32::is_finite)
            && self.half_height > 0.
            && self.near >= 0.
            && self.far > self.near
            && self.field_of_view.is_finite()
            && self.field_of_view > 0.
            && self.field_of_view < 180.;
        let viewport_valid = self.viewport.into_iter().all(f32::is_finite)
            && self.viewport[2] >= 0.
            && self.viewport[3] >= 0.;
        let matrices_valid = self
            .projection_matrix
            .iter()
            .chain(&self.world_to_camera)
            .all(|matrix| matrix.iter().all(|v| v.is_finite()));
        let background_valid = self
            .background
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v));
        let flags_valid = self.camera_type != 0
            && self.camera_type & !CAMERA_TYPE_BITS == 0
            && self.depth_texture_mode & !DEPTH_TEXTURE_BITS == 0
            && self.target_display < DISPLAY_COUNT;
        if lens_valid && viewport_valid && matrices_valid && background_valid && flags_valid {
            Ok(())
        } else {
            Err(invalid())
        }
    }

    /// Rejects the settings a frame cannot show yet, each by the Unity member that sets it.
    pub fn require_presentable(self) -> Result<(), WorldError> {
        if self.projection == Projection::Perspective {
            return Err(lighting::unsupported("perspective projection"));
        }
        if self.projection_matrix.is_some() {
            return Err(lighting::unsupported("projectionMatrix"));
        }
        if self.world_to_camera.is_some() {
            return Err(lighting::unsupported("worldToCameraMatrix"));
        }
        if self.target_texture != 0 {
            return Err(lighting::unsupported("targetTexture"));
        }
        if self.target_display != 0 {
            return Err(lighting::unsupported("targetDisplay"));
        }
        if !self.fills_screen() {
            return Err(lighting::unsupported("rect"));
        }
        Ok(())
    }

    fn fills_screen(self) -> bool {
        let [x, y, width, height] = self.viewport;
        x.abs() <= f32::EPSILON
            && y.abs() <= f32::EPSILON
            && (width - 1.).abs() <= f32::EPSILON
            && (height - 1.).abs() <= f32::EPSILON
    }

    /// Whether the camera draws objects on `layer`.
    pub fn sees_layer(self, layer: u8) -> bool {
        self.culling_mask & 1u32.checked_shl(u32::from(layer)).unwrap_or(0) != 0
    }

    /// The colour a frame starts from. Settings the frame cannot show, and clear modes that keep a
    /// lower camera's output, are rejected, because a frame has one camera.
    pub fn clear_color(self) -> Result<[f64; 4], WorldError> {
        self.require_presentable()?;
        match self.clear {
            CameraClear::Skybox | CameraClear::SolidColor => Ok(self.background),
            CameraClear::DepthOnly => Err(lighting::unsupported("clearFlags Depth")),
            CameraClear::Nothing => Err(lighting::unsupported("clearFlags Nothing")),
        }
    }
}
