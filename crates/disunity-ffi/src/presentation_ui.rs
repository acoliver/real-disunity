//! Typed pixel-space UI belonging to the existing prepared presentation.
#[cfg(test)]
#[path = "presentation_ui_tests.rs"]
mod tests;

use crate::{
    DisunityResult,
    callbacks::mutate,
    presentation::{DisunityCoverage, DisunityQuad},
};
use worldrunner_world_simulation::ui::presentation::{UiDraw, UiDrawKind};

fn draw_order(draw: &UiDraw) -> (i32, i32, usize, UiDrawKind) {
    (
        draw.sorting_layer,
        draw.sorting_order,
        draw.hierarchy_order,
        draw.kind,
    )
}

fn validate_draw(draw: &UiDraw, previous: Option<&UiDraw>) -> Result<(), crate::errors::AbiError> {
    if previous.is_some_and(|before| draw_order(before) > draw_order(draw)) {
        return Err("presentation UI draw order is descending".into());
    }
    if draw.corner_colors.is_some_and(|colors| {
        colors
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
    }) {
        return Err("presentation UI corner_colors must be finite channels in 0..=1".into());
    }
    let [x, y, width, height] = draw.clip_rect;
    if x.checked_add(width).is_none() || y.checked_add(height).is_none() {
        return Err("presentation UI clip bounds overflow".into());
    }
    Ok(())
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct DisunityUiDraw {
    /// Low two bits: 0 quad graphic, 1 text, 2 triangle. Bit 2 enables scene-depth testing.
    pub kind: u32,
    pub hierarchy_order: u64,
    pub clip_x: u32,
    pub clip_y: u32,
    pub clip_width: u32,
    pub clip_height: u32,
    /// Top-row-origin UV pairs in corner order for triangles; zero for quad graphics and text.
    pub triangle_u0: f32,
    pub triangle_v0: f32,
    pub triangle_u1: f32,
    pub triangle_v1: f32,
    pub triangle_u2: f32,
    pub triangle_v2: f32,
    /// Zero for white corners, one for explicit linear RGBA in BL/BR/TR/TL order.
    pub corner_colors_present: u32,
    pub corner_0_r: f32,
    pub corner_0_g: f32,
    pub corner_0_b: f32,
    pub corner_0_a: f32,
    pub corner_1_r: f32,
    pub corner_1_g: f32,
    pub corner_1_b: f32,
    pub corner_1_a: f32,
    pub corner_2_r: f32,
    pub corner_2_g: f32,
    pub corner_2_b: f32,
    pub corner_2_a: f32,
    pub corner_3_r: f32,
    pub corner_3_g: f32,
    pub corner_3_b: f32,
    pub corner_3_a: f32,
}

/// # Safety
/// Both outputs must be writable for one count.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_ui_count(
    world: u64,
    draws: *mut u32,
    textures: *mut u32,
) -> DisunityResult {
    if draws.is_null() || textures.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ui = s
            .ui_presentation
            .as_ref()
            .ok_or("presentation is not prepared")?;
        for (index, draw) in ui.draws.iter().enumerate() {
            validate_draw(draw, index.checked_sub(1).and_then(|i| ui.draws.get(i)))?;
        }
        let count = u32::try_from(ui.draws.len()).map_err(|_| "UI draw count overflow")?;
        let coverage = u32::try_from(ui.coverage.len()).map_err(|_| "UI texture count overflow")?;
        unsafe {
            *draws = count;
            *textures = coverage;
        }
        Ok(())
    })
}

/// Geometry is in lower-left viewport pixels, not the live 2D frame's world-space quads.
/// Layer/order are in `quad`; hierarchy and scissor bounds are in `output`.
/// # Safety
/// Each output must be writable for one record.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_ui_draw(
    world: u64,
    index: u32,
    output: *mut DisunityUiDraw,
    quad: *mut DisunityQuad,
) -> DisunityResult {
    if output.is_null() || quad.is_null() {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let ui = s
            .ui_presentation
            .as_ref()
            .ok_or("presentation is not prepared")?;
        let index = index as usize;
        let draw = ui
            .draws
            .get(index)
            .ok_or("presentation UI draw index out of range")?;
        validate_draw(draw, index.checked_sub(1).and_then(|i| ui.draws.get(i)))?;
        unsafe {
            *output = encode_metadata(draw);
            *quad = encode(draw);
        }
        Ok(())
    })
}

/// # Safety
/// Metadata is writable; bytes is writable for capacity, or null when capacity is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn disunity_presentation_ui_coverage(
    world: u64,
    index: u32,
    output: *mut DisunityCoverage,
    bytes: *mut u8,
    capacity: u32,
) -> DisunityResult {
    if output.is_null() || (capacity != 0 && bytes.is_null()) {
        return DisunityResult::invalid_argument();
    }
    mutate(world, |s| {
        let (identity, texture) = s
            .ui_presentation
            .as_ref()
            .and_then(|ui| ui.coverage.iter().nth(index as usize))
            .ok_or("presentation UI texture index out of range")?;
        unsafe { crate::presentation::copy_coverage(*identity, texture, output, bytes, capacity) }
    })
}

fn encode_metadata(draw: &UiDraw) -> DisunityUiDraw {
    let [clip_x, clip_y, clip_width, clip_height] = draw.clip_rect;
    let [
        [triangle_u0, triangle_v0],
        [triangle_u1, triangle_v1],
        [triangle_u2, triangle_v2],
    ] = draw.triangle_uv.unwrap_or([[0.; 2]; 3]);
    let [
        [corner_0_r, corner_0_g, corner_0_b, corner_0_a],
        [corner_1_r, corner_1_g, corner_1_b, corner_1_a],
        [corner_2_r, corner_2_g, corner_2_b, corner_2_a],
        [corner_3_r, corner_3_g, corner_3_b, corner_3_a],
    ] = draw.corner_colors.unwrap_or([[1.; 4]; 4]);
    DisunityUiDraw {
        kind: (if draw.triangle_uv.is_some() {
            2
        } else {
            match draw.kind {
                UiDrawKind::Graphic => 0,
                UiDrawKind::Text => 1,
            }
        }) | (u32::from(draw.depth_test) << 2),
        hierarchy_order: draw.hierarchy_order as u64,
        clip_x,
        clip_y,
        clip_width,
        clip_height,
        triangle_u0,
        triangle_v0,
        triangle_u1,
        triangle_v1,
        triangle_u2,
        triangle_v2,
        corner_colors_present: u32::from(draw.corner_colors.is_some()),
        corner_0_r,
        corner_0_g,
        corner_0_b,
        corner_0_a,
        corner_1_r,
        corner_1_g,
        corner_1_b,
        corner_1_a,
        corner_2_r,
        corner_2_g,
        corner_2_b,
        corner_2_a,
        corner_3_r,
        corner_3_g,
        corner_3_b,
        corner_3_a,
    }
}

fn encode(draw: &UiDraw) -> DisunityQuad {
    let c = draw.corners;
    DisunityQuad {
        texture: draw.texture.unwrap_or(0),
        textured: u32::from(draw.texture.is_some()),
        overlay: 1,
        sorting_layer: draw.sorting_layer,
        sorting_order: draw.sorting_order,
        p0_x: c[0][0],
        p0_y: c[0][1],
        p0_z: c[0][2],
        p1_x: c[1][0],
        p1_y: c[1][1],
        p1_z: c[1][2],
        p2_x: c[2][0],
        p2_y: c[2][1],
        p2_z: c[2][2],
        p3_x: c[3][0],
        p3_y: c[3][1],
        p3_z: c[3][2],
        u0: draw.uv_rect[0],
        v0: draw.uv_rect[1],
        u1: draw.uv_rect[2],
        v1: draw.uv_rect[3],
        r: draw.color[0],
        g: draw.color[1],
        b: draw.color[2],
        a: draw.color[3],
        ..Default::default()
    }
}
