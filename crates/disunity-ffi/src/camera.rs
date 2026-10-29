//! Camera component settings: projection size, clip planes, clear mode and draw order.
use crate::{DisunityResult, callbacks::mutate, physics::component};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentHandle,
    presentation::{Camera, CameraClear, CameraFlags, Projection},
};

/// A camera's serialized settings. `clear_flags` uses Unity's `CameraClearFlags` values:
/// 1 Skybox, 2 SolidColor, 3 Depth, 4 Nothing. `orthographic` is 1 for an orthographic lens and 0
/// for a perspective one with `field_of_view` degrees; both sizes are kept. `culling_mask` is one
/// bit per layer, the viewport is a normalized rectangle, `depth_texture_mode` and `camera_type`
/// use Unity's flag values, and `target_texture` is a managed RenderTexture's instance id (0 for
/// the screen). Custom matrices have their own calls. Boolean settings are 0 or 1.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct DisunityCamera {
    pub enabled: u32,
    pub clear_flags: u32,
    pub orthographic: u32,
    pub depth: i32,
    pub orthographic_size: f32,
    pub near: f32,
    pub far: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
    pub field_of_view: f32,
    pub target_texture: u64,
    pub culling_mask: u32,
    pub viewport_x: f32,
    pub viewport_y: f32,
    pub viewport_width: f32,
    pub viewport_height: f32,
    pub depth_texture_mode: u32,
    pub allow_hdr: u32,
    pub allow_msaa: u32,
    pub allow_dynamic_resolution: u32,
    pub use_occlusion_culling: u32,
    pub target_display: u32,
    pub camera_type: u32,
}

impl Default for DisunityCamera {
    /// The settings of a new camera component.
    fn default() -> Self {
        Self::from_native(Camera::default())
    }
}

impl DisunityCamera {
    fn from_native(camera: Camera) -> Self {
        let [r, g, b, a] = camera.background.map(|channel| channel as f32);
        let [viewport_x, viewport_y, viewport_width, viewport_height] = camera.viewport;
        let flag = |flag| u32::from(camera.flags.contains(flag));
        Self {
            enabled: u32::from(camera.enabled),
            clear_flags: camera.clear as u32,
            orthographic: u32::from(camera.projection == Projection::Orthographic),
            depth: camera.depth,
            orthographic_size: camera.half_height,
            near: camera.near,
            far: camera.far,
            r,
            g,
            b,
            a,
            field_of_view: camera.field_of_view,
            target_texture: camera.target_texture,
            culling_mask: camera.culling_mask,
            viewport_x,
            viewport_y,
            viewport_width,
            viewport_height,
            depth_texture_mode: camera.depth_texture_mode,
            allow_hdr: flag(CameraFlags::HDR),
            allow_msaa: flag(CameraFlags::MSAA),
            allow_dynamic_resolution: flag(CameraFlags::DYNAMIC_RESOLUTION),
            use_occlusion_culling: flag(CameraFlags::OCCLUSION_CULLING),
            target_display: camera.target_display,
            camera_type: camera.camera_type,
        }
    }

    /// The camera these settings describe, keeping `current`'s custom matrices.
    fn to_native(self, current: Camera) -> Result<Camera, crate::errors::AbiError> {
        let projection = match self.orthographic {
            1 => Projection::Orthographic,
            0 => Projection::Perspective,
            _ => return Err("camera orthographic must be 0 or 1".into()),
        };
        let booleans = [
            self.enabled,
            self.allow_hdr,
            self.allow_msaa,
            self.allow_dynamic_resolution,
            self.use_occlusion_culling,
        ];
        if booleans.iter().any(|value| *value > 1) {
            return Err("camera boolean settings must be 0 or 1".into());
        }
        let clear =
            CameraClear::from_flags(self.clear_flags).ok_or("unknown camera clear flags")?;
        let flags = [
            (CameraFlags::HDR, self.allow_hdr),
            (CameraFlags::MSAA, self.allow_msaa),
            (
                CameraFlags::DYNAMIC_RESOLUTION,
                self.allow_dynamic_resolution,
            ),
            (CameraFlags::OCCLUSION_CULLING, self.use_occlusion_culling),
        ]
        .into_iter()
        .fold(current.flags, |flags, (flag, on)| flags.with(flag, on == 1));
        let camera = Camera {
            enabled: self.enabled == 1,
            projection,
            half_height: self.orthographic_size,
            field_of_view: self.field_of_view,
            near: self.near,
            far: self.far,
            clear,
            background: [self.r, self.g, self.b, self.a].map(f64::from),
            depth: self.depth,
            culling_mask: self.culling_mask,
            viewport: [
                self.viewport_x,
                self.viewport_y,
                self.viewport_width,
                self.viewport_height,
            ],
            depth_texture_mode: self.depth_texture_mode,
            flags,
            target_display: self.target_display,
            target_texture: self.target_texture,
            camera_type: self.camera_type,
            ..current
        };
        camera.validate()?;
        Ok(camera)
    }
}

fn camera(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, Camera), crate::errors::AbiError> {
    let id = component(session, handle)?;
    match session.native.component(id)?.desc {
        ComponentDesc::Camera(camera) => Ok((id, camera)),
        _ => Err("component is not a Camera".into()),
    }
}

/// # Safety
/// Output is writable for one Camera record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_read(
    world: u64,
    handle: u64,
    output: *mut DisunityCamera,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = camera(s, handle)?;
        unsafe { *output = DisunityCamera::from_native(value) };
        Ok(())
    })
}

/// Validates every setting before changing the camera. Custom matrices are left as they are.
/// # Safety
/// Value is readable for one Camera record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_write(
    world: u64,
    handle: u64,
    value: *const DisunityCamera,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let (id, current) = camera(s, handle)?;
        s.native
            .set_component(id, ComponentDesc::Camera(value.to_native(current)?))?;
        Ok(())
    })
}

const MATRIX_PROJECTION: u32 = 0;
const MATRIX_WORLD_TO_CAMERA: u32 = 1;

fn matrix_slot(
    camera: &mut Camera,
    kind: u32,
) -> Result<&mut Option<[f32; 16]>, crate::errors::AbiError> {
    match kind {
        MATRIX_PROJECTION => Ok(&mut camera.projection_matrix),
        MATRIX_WORLD_TO_CAMERA => Ok(&mut camera.world_to_camera),
        _ => Err("unknown camera matrix kind".into()),
    }
}

/// Reads a custom matrix as 16 column-major floats. `is_set` is 0, with `matrix` untouched, when
/// the camera derives that matrix from its settings and transform.
/// `kind` is 0 for the projection matrix and 1 for the world-to-camera matrix.
/// # Safety
/// `matrix` is writable for 16 floats and `is_set` for one `u32`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_matrix_read(
    world: u64,
    handle: u64,
    kind: u32,
    matrix: *mut f32,
    is_set: *mut u32,
) -> DisunityResult {
    if matrix.is_null() || is_set.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, mut value) = camera(s, handle)?;
        let slot = *matrix_slot(&mut value, kind)?;
        unsafe {
            *is_set = u32::from(slot.is_some());
            if let Some(values) = slot {
                std::ptr::copy_nonoverlapping(values.as_ptr(), matrix, values.len());
            }
        }
        Ok(())
    })
}

/// Sets a custom matrix from 16 column-major floats, or clears it when `matrix` is null.
/// Nothing changes when a value is not finite. Frames reject a camera with a custom matrix.
/// # Safety
/// `matrix` is null or readable for 16 floats.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_camera_matrix_write(
    world: u64,
    handle: u64,
    kind: u32,
    matrix: *const f32,
) -> DisunityResult {
    let values = if matrix.is_null() {
        None
    } else {
        let mut values = [0.; 16];
        unsafe { std::ptr::copy_nonoverlapping(matrix, values.as_mut_ptr(), values.len()) };
        Some(values)
    };
    mutate(world, |s| {
        let (id, mut value) = camera(s, handle)?;
        *matrix_slot(&mut value, kind)? = values;
        value.validate()?;
        s.native.set_component(id, ComponentDesc::Camera(value))?;
        Ok(())
    })
}
