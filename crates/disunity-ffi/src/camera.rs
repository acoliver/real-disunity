//! Camera component settings: projection size, clip planes, clear mode and draw order.
use crate::{DisunityResult, callbacks::mutate, physics::component};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ComponentHandle, WorldError,
    presentation::{Camera, CameraClear},
};

/// A camera's serialized settings. `clear_flags` uses Unity's `CameraClearFlags` values:
/// 1 Skybox, 2 SolidColor, 3 Depth, 4 Nothing. Only orthographic cameras are supported.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
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
}

impl DisunityCamera {
    fn from_native(camera: Camera) -> Self {
        let [r, g, b, a] = camera.background.map(|channel| channel as f32);
        Self {
            enabled: u32::from(camera.enabled),
            clear_flags: camera.clear as u32,
            orthographic: 1,
            depth: camera.depth,
            orthographic_size: camera.half_height,
            near: camera.near,
            far: camera.far,
            r,
            g,
            b,
            a,
        }
    }

    fn to_native(self) -> Result<Camera, crate::errors::AbiError> {
        if self.orthographic != 1 {
            return Err(unsupported("orthographic"));
        }
        if self.enabled > 1 {
            return Err("camera enabled must be 0 or 1".into());
        }
        let clear =
            CameraClear::from_flags(self.clear_flags).ok_or("unknown camera clear flags")?;
        let camera = Camera {
            enabled: self.enabled == 1,
            half_height: self.orthographic_size,
            near: self.near,
            far: self.far,
            clear,
            background: [self.r, self.g, self.b, self.a].map(f64::from),
            depth: self.depth,
        };
        camera.validate()?;
        Ok(camera)
    }
}

fn unsupported(field: &'static str) -> crate::errors::AbiError {
    WorldError::InvalidComponent(ComponentError::UnsupportedField(field)).into()
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

/// Validates every setting before changing the camera. Perspective projection is reported as
/// an unsupported `orthographic` field.
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
        let (id, _) = camera(s, handle)?;
        s.native
            .set_component(id, ComponentDesc::Camera(value.to_native()?))?;
        Ok(())
    })
}
