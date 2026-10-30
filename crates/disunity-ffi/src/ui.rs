//! Native UI component access. Font bytes are explicit resources, never substituted.
use crate::{
    DisunityResult,
    callbacks::mutate,
    physics::{component, create},
};
use glam::{UVec2, Vec2};
use worldrunner_world_simulation::{
    ComponentDesc,
    ui::{Canvas, CanvasMode, RectTransform, TextRun},
};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityRect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
    pub pivot_x: f32,
    pub pivot_y: f32,
    pub size_x: f32,
    pub size_y: f32,
    pub position_x: f32,
    pub position_y: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityCanvas {
    pub mode: u32,
    pub enabled: u32,
    pub camera: u64,
    pub orthographic_size: f32,
    pub plane_distance: f32,
    pub sorting_order: i32,
    pub sorting_layer: i32,
    pub override_sorting: u32,
    pub target_display: u32,
    pub additional_shader_channels: u32,
    pub reference_pixels_per_unit: f32,
}

impl DisunityCanvas {
    fn from_native(c: Canvas) -> Self {
        Self {
            enabled: u32::from(c.enabled),
            sorting_order: c.sorting_order,
            sorting_layer: c.sorting_layer,
            override_sorting: u32::from(c.override_sorting),
            target_display: c.target_display,
            additional_shader_channels: c.additional_shader_channels,
            reference_pixels_per_unit: c.reference_pixels_per_unit,
            ..Self::default()
        }
    }
}

/// # Safety
/// Bytes are readable for length; output is writable for one identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_font_register(
    world: u64,
    bytes: *const u8,
    length: u32,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() || bytes.is_null() || length > 8 * 1024 * 1024 {
        return DisunityResult::invalid_argument();
    }
    let bytes = unsafe { std::slice::from_raw_parts(bytes, length as usize) }.to_vec();
    mutate(world, |s| {
        let next = s
            .font_ids
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("font identity exhausted")?;
        let id = s
            .fonts
            .register(bytes)
            .map_err(crate::errors::AbiError::from)?;
        s.font_ids.insert(next, id);
        unsafe { *output = next };
        Ok(())
    })
}

/// # Safety
/// Output is writable for one component identity.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_ui_create(
    world: u64,
    object: u64,
    kind: u32,
    font: u64,
    output: *mut u64,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    let desc = match kind {
        0 => ComponentDesc::RectTransform(RectTransform::default()),
        1 => ComponentDesc::Canvas(Canvas::default()),
        2 => {
            let store = crate::lock_store();
            let Ok(s) = store.session(world) else {
                return DisunityResult::invalid_handle();
            };
            let Some(&font) = s.font_ids.get(&font) else {
                return DisunityResult::invalid_argument();
            };
            ComponentDesc::Text(TextRun::new(font))
        }
        _ => return DisunityResult::invalid_argument(),
    };
    match create(world, object, desc) {
        Ok(id) => {
            unsafe { *output = id };
            DisunityResult::ok()
        }
        Err(e) => crate::abi::translate_error(Some(world), &e),
    }
}

/// # Safety
/// Value is readable for one rectangle record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_rect_write(
    world: u64,
    handle: u64,
    value: *const DisunityRect,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let v = unsafe { *value };
    mutate(world, |s| {
        let id = component(s, handle)?;
        if !matches!(
            s.native
                .component(id)
                .map_err(crate::errors::AbiError::from)?
                .desc,
            ComponentDesc::RectTransform(_)
        ) {
            return Err("component is not a rectangle".into());
        }
        s.native
            .set_component(
                id,
                ComponentDesc::RectTransform(RectTransform {
                    anchor_min: Vec2::new(v.min_x, v.min_y),
                    anchor_max: Vec2::new(v.max_x, v.max_y),
                    pivot: Vec2::new(v.pivot_x, v.pivot_y),
                    size_delta: Vec2::new(v.size_x, v.size_y),
                    anchored_position: Vec2::new(v.position_x, v.position_y),
                }),
            )
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// Output is writable for one rectangle record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_rect_read(
    world: u64,
    handle: u64,
    output: *mut DisunityRect,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ComponentDesc::RectTransform(r) = s
            .native
            .component(component(s, handle)?)
            .map_err(crate::errors::AbiError::from)?
            .desc
        else {
            return Err("component is not a rectangle".into());
        };
        unsafe {
            *output = DisunityRect {
                min_x: r.anchor_min.x,
                min_y: r.anchor_min.y,
                max_x: r.anchor_max.x,
                max_y: r.anchor_max.y,
                pivot_x: r.pivot.x,
                pivot_y: r.pivot.y,
                size_x: r.size_delta.x,
                size_y: r.size_delta.y,
                position_x: r.anchored_position.x,
                position_y: r.anchored_position.y,
            }
        };
        Ok(())
    })
}

/// Applies a canvas record to `current`, keeping its scale and pixel-perfect flag, which the
/// record does not carry.
fn canvas(
    s: &crate::store::Session,
    current: Canvas,
    v: DisunityCanvas,
) -> Result<Canvas, crate::errors::AbiError> {
    if v.enabled > 1 || v.override_sorting > 1 {
        return Err("invalid Canvas flags".into());
    }
    let mode = match v.mode {
        0 => CanvasMode::ScreenSpaceOverlay,
        1 => CanvasMode::ScreenSpaceCamera {
            camera: s.object(v.camera)?,
            orthographic_size: v.orthographic_size,
            plane_distance: v.plane_distance,
        },
        2 => CanvasMode::WorldSpace,
        _ => return Err("invalid Canvas mode".into()),
    };
    Ok(Canvas {
        enabled: v.enabled == 1,
        mode,
        sorting_order: v.sorting_order,
        sorting_layer: v.sorting_layer,
        override_sorting: v.override_sorting == 1,
        target_display: v.target_display,
        additional_shader_channels: v.additional_shader_channels,
        reference_pixels_per_unit: v.reference_pixels_per_unit,
        ..current
    })
}

/// # Safety
/// Value is readable for one Canvas record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_write(
    world: u64,
    handle: u64,
    value: *const DisunityCanvas,
) -> DisunityResult {
    if value.is_null() {
        return DisunityResult::invalid_argument();
    }
    let v = unsafe { *value };
    mutate(world, |s| {
        let (id, current) = crate::canvas::canvas(s, handle)?;
        s.native
            .set_component(id, ComponentDesc::Canvas(canvas(s, current, v)?))
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// Output is writable for one Canvas record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_canvas_read(
    world: u64,
    handle: u64,
    output: *mut DisunityCanvas,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ComponentDesc::Canvas(c) = s
            .native
            .component(component(s, handle)?)
            .map_err(crate::errors::AbiError::from)?
            .desc
        else {
            return Err("component is not a Canvas".into());
        };
        let mut v = DisunityCanvas::from_native(c);
        match c.mode {
            CanvasMode::ScreenSpaceOverlay => {}
            CanvasMode::WorldSpace => v.mode = 2,
            CanvasMode::ScreenSpaceCamera {
                camera,
                orthographic_size,
                plane_distance,
            } => {
                v.mode = 1;
                v.camera = s
                    .objects
                    .iter()
                    .find_map(|(&id, &o)| (o == camera).then_some(id))
                    .ok_or("camera binding missing")?;
                v.orthographic_size = orthographic_size;
                v.plane_distance = plane_distance;
            }
        }
        unsafe { *output = v };
        Ok(())
    })
}

/// # Safety
/// Bytes are readable for length; null is permitted only for empty text.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_text_write(
    world: u64,
    handle: u64,
    size: f32,
    bytes: *const u8,
    length: u32,
) -> DisunityResult {
    if length > 65536 || (bytes.is_null() && length != 0) {
        return DisunityResult::invalid_argument();
    }
    let bytes = if length == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(bytes, length as usize) }
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return DisunityResult::invalid_argument();
    };
    mutate(world, |s| {
        let id = component(s, handle)?;
        let ComponentDesc::Text(mut run) = s
            .native
            .component(id)
            .map_err(crate::errors::AbiError::from)?
            .desc
            .clone()
        else {
            return Err("component is not text".into());
        };
        run.text = text.to_owned();
        run.font_size = size;
        s.native
            .set_component(id, ComponentDesc::Text(run))
            .map_err(crate::errors::AbiError::from)
    })
}

/// # Safety
/// Buffer is writable for capacity, written and size for one value each.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_text_read(
    world: u64,
    handle: u64,
    buffer: *mut u8,
    capacity: u32,
    written: *mut u32,
    size: *mut f32,
) -> DisunityResult {
    if written.is_null() || size.is_null() || (buffer.is_null() && capacity != 0) {
        return DisunityResult::invalid_argument();
    }
    let store = crate::lock_store();
    let Ok(s) = store.session(world) else {
        return DisunityResult::invalid_handle();
    };
    let Ok(id) = component(s, handle) else {
        return DisunityResult::invalid_handle();
    };
    let Ok(c) = s.native.component(id) else {
        return DisunityResult::invalid_handle();
    };
    let ComponentDesc::Text(run) = &c.desc else {
        return DisunityResult::invalid_argument();
    };
    let bytes = run.text.as_bytes();
    unsafe {
        *written = bytes.len() as u32;
        *size = run.font_size
    };
    if (capacity as usize) < bytes.len() {
        return DisunityResult { code: 2, detail: 0 };
    }
    if !bytes.is_empty() {
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len()) };
    }
    DisunityResult::ok()
}

/// # Safety
/// Output is writable for one rectangle record. Size and position fields carry resolved bounds.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_ui_rect(
    world: u64,
    object: u64,
    width: u32,
    height: u32,
    output: *mut DisunityRect,
) -> DisunityResult {
    if output.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let layout = s
            .native
            .ui_layout(UVec2::new(width, height))
            .map_err(crate::errors::AbiError::from)?;
        let r = layout
            .rect(s.object(object)?)
            .ok_or("UI rectangle has no visible Canvas")?;
        unsafe {
            *output = DisunityRect {
                size_x: r.size.x,
                size_y: r.size.y,
                position_x: -r.pivot.x * r.size.x,
                position_y: -r.pivot.y * r.size.y,
                ..Default::default()
            }
        };
        Ok(())
    })
}
