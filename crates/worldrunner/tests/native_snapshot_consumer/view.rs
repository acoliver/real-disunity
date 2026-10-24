use worldrunner::{
    glam::DVec3,
    scene::camera::{
        CameraLens, CameraPose, CameraState, ResolvedView, ViewConvention, ViewHandedness,
        Viewport, resolve,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub viewport: [u32; 2],
    pub resize_sequence: u64,
}

impl Target {
    pub fn new() -> Self {
        Self {
            viewport: [65, 47],
            resize_sequence: 0,
        }
    }

    pub fn resize(&mut self, size: [u32; 2]) {
        if self.viewport != size {
            self.viewport = size;
            self.resize_sequence = self.resize_sequence.checked_add(1).unwrap();
        }
    }

    pub fn resolve(&self) -> ResolvedView {
        resolve(&CameraState {
            pose: CameraPose {
                eye_meters: DVec3::new(0.0, -10.0, 0.0),
                forward: DVec3::Y,
                up: DVec3::Z,
            },
            lens: CameraLens::Orthographic {
                half_height_meters: 2.0,
                near_meters: 0.0,
                far_meters: 20.0,
            },
            viewport: Viewport {
                width: self.viewport[0],
                height: self.viewport[1],
            },
            convention: ViewConvention {
                handedness: ViewHandedness::RightHanded,
            },
        })
        .unwrap()
    }
}

pub const POLICY: &str = "native XYZ to canonical XZY; identity rotation/scale; RH ortho half-height 2; eye (0,-10,0); forward +Y; up +Z";

pub fn canonical_position(native: [f32; 3]) -> [f32; 3] {
    [native[0], native[2], native[1]]
}
