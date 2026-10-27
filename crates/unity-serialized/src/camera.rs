//! The `Camera` and `Light` layouts.

use crate::{
    ClassId, SerializedError,
    object::{Layout, ObjectReader, PPtr},
};

/// Physical camera properties, used when the camera simulates a real lens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalCamera {
    pub gate_fit_mode: i32,
    pub iso: i32,
    pub shutter_speed: f32,
    pub aperture: f32,
    pub focus_distance: f32,
    pub focal_length: f32,
    pub blade_count: i32,
    pub curvature: [f32; 2],
    pub barrel_clipping: f32,
    pub anamorphism: f32,
    pub sensor_size: [f32; 2],
    pub lens_shift: [f32; 2],
}

impl PhysicalCamera {
    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        Ok(Self {
            gate_fit_mode: r.i32()?,
            iso: r.i32()?,
            shutter_speed: r.f32()?,
            aperture: r.f32()?,
            focus_distance: r.f32()?,
            focal_length: r.f32()?,
            blade_count: r.i32()?,
            curvature: r.floats()?,
            barrel_clipping: r.f32()?,
            anamorphism: r.f32()?,
            sensor_size: r.floats()?,
            lens_shift: r.floats()?,
        })
    }
}

/// How a camera projects: clip planes and either a perspective field of view or an
/// orthographic size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projection {
    pub near: f32,
    pub far: f32,
    /// Vertical field of view in degrees.
    pub field_of_view: f32,
    pub orthographic: bool,
    /// Half the vertical size of the orthographic view.
    pub orthographic_size: f32,
}

/// Where and how a camera renders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CameraOutput {
    pub rendering_path: i32,
    /// Null when the camera renders to a display.
    pub target_texture: PPtr,
    pub target_display: i32,
    pub target_eye: i32,
    pub hdr: bool,
    pub allow_msaa: bool,
    pub allow_dynamic_resolution: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Camera {
    pub game_object: PPtr,
    pub enabled: bool,
    /// Unity's `CameraClearFlags`: 1 skybox, 2 solid color, 3 depth only, 4 nothing.
    pub clear_flags: u32,
    /// Clear color as r, g, b, a.
    pub background_color: [f32; 4],
    pub projection_matrix_mode: i32,
    pub physical: PhysicalCamera,
    /// Viewport as x, y, width and height, normalized to the target.
    pub viewport: [f32; 4],
    pub projection: Projection,
    pub depth: f32,
    pub culling_mask: u32,
    pub output: CameraOutput,
    pub force_into_render_texture: bool,
    pub occlusion_culling: bool,
    pub stereo_convergence: f32,
    pub stereo_separation: f32,
}

impl Camera {
    fn read_projection(r: &mut ObjectReader<'_>) -> Result<Projection, SerializedError> {
        let [near, far, field_of_view] = r.floats()?;
        let orthographic = r.bool()?;
        r.align()?;
        Ok(Projection {
            near,
            far,
            field_of_view,
            orthographic,
            orthographic_size: r.f32()?,
        })
    }
}

impl Layout for Camera {
    const CLASS: ClassId = ClassId::CAMERA;

    fn read(r: &mut ObjectReader<'_>) -> Result<Self, SerializedError> {
        let game_object = r.pptr()?;
        let enabled = r.bool()?;
        r.align()?;
        let clear_flags = r.u32()?;
        let background_color = r.floats()?;
        let projection_matrix_mode = r.i32()?;
        let physical = PhysicalCamera::read(r)?;
        let viewport = r.floats()?;
        let projection = Self::read_projection(r)?;
        let depth = r.f32()?;
        let culling_mask = r.u32()?;
        let output = CameraOutput {
            rendering_path: r.i32()?,
            target_texture: r.pptr()?,
            target_display: r.i32()?,
            target_eye: r.i32()?,
            hdr: r.bool()?,
            allow_msaa: r.bool()?,
            allow_dynamic_resolution: r.bool()?,
        };
        let [force_into_render_texture, occlusion_culling] = [r.bool()?, r.bool()?];
        r.align()?;
        Ok(Self {
            game_object,
            enabled,
            clear_flags,
            background_color,
            projection_matrix_mode,
            physical,
            viewport,
            projection,
            depth,
            culling_mask,
            output,
            force_into_render_texture,
            occlusion_culling,
            stereo_convergence: r.f32()?,
            stereo_separation: r.f32()?,
        })
    }
}
