//! Canvas scaling, the settings Unity's `CanvasScaler` serializes.
use crate::{DisunityResult, callbacks::mutate, physics::component};
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::{
    ComponentDesc, ComponentError, ComponentHandle, WorldError,
    ui::{Canvas, CanvasScale},
};

/// How a canvas scales its layout. `mode` and `screen_match` use Unity's `ScaleMode` and
/// `ScreenMatchMode` values. Supported: constant pixel size (0) with `factor`, and scale with
/// screen size (1) matching width or height (0) against `reference_x` by `reference_y`, where
/// `match_height` runs from 0 (match width) to 1 (match height).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCanvasScale {
    pub mode: u32,
    pub screen_match: u32,
    pub factor: f32,
    pub reference_x: f32,
    pub reference_y: f32,
    pub match_height: f32,
}

impl DisunityCanvasScale {
    fn from_native(scale: CanvasScale) -> Self {
        match scale {
            CanvasScale::Constant(factor) => Self {
                factor,
                ..Self::default()
            },
            CanvasScale::MatchViewport {
                reference,
                match_height,
            } => Self {
                mode: 1,
                factor: 1.,
                reference_x: reference.x,
                reference_y: reference.y,
                match_height,
                ..Self::default()
            },
        }
    }

    fn to_native(self) -> Result<CanvasScale, crate::errors::AbiError> {
        match (self.mode, self.screen_match) {
            (0, _) => Ok(CanvasScale::Constant(self.factor)),
            (1, 0) => Ok(CanvasScale::MatchViewport {
                reference: Vec2::new(self.reference_x, self.reference_y),
                match_height: self.match_height,
            }),
            (1, 1 | 2) => Err(unsupported("screenMatchMode")),
            (2, _) => Err(unsupported("uiScaleMode ConstantPhysicalSize")),
            _ => Err("unknown canvas scale mode".into()),
        }
    }
}

fn unsupported(field: &'static str) -> crate::errors::AbiError {
    WorldError::InvalidComponent(ComponentError::UnsupportedField(field)).into()
}

pub(crate) fn canvas(
    session: &crate::store::Session,
    handle: u64,
) -> Result<(ComponentHandle, Canvas), crate::errors::AbiError> {
    let id = component(session, handle)?;
    match session.native.component(id)?.desc {
        ComponentDesc::Canvas(canvas) => Ok((id, canvas)),
        _ => Err("component is not a Canvas".into()),
    }
}

/// # Safety
/// Output is writable for one canvas scale record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_scale_read(
    world: u64,
    handle: u64,
    output: *mut DisunityCanvasScale,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = canvas(s, handle)?;
        unsafe { *output = DisunityCanvasScale::from_native(value.scale) };
        Ok(())
    })
}

/// Validates the scale before changing the canvas.
/// # Safety
/// Value is readable for one canvas scale record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_scale_write(
    world: u64,
    handle: u64,
    value: *const DisunityCanvasScale,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let value = unsafe { *value };
    mutate(world, |s| {
        let (id, mut target) = canvas(s, handle)?;
        target.scale = value.to_native()?;
        s.native.set_component(id, ComponentDesc::Canvas(target))?;
        Ok(())
    })
}

/// The factor the canvas scales its layout by on a `width` by `height` viewport, which is
/// Unity's `Canvas.scaleFactor` for that screen.
/// # Safety
/// Output is writable for one factor.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_scale_factor(
    world: u64,
    handle: u64,
    width: u32,
    height: u32,
    output: *mut f32,
) -> DisunityResult {
    if output.is_null() || width == 0 || height == 0 {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (_, value) = canvas(s, handle)?;
        let factor = value.scale.factor(UVec2::new(width, height))?;
        unsafe { *output = factor };
        Ok(())
    })
}
