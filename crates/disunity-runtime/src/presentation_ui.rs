//! Copies detached UI from the prepared frame in the host's loaded engine image.
#[cfg(test)]
#[path = "presentation_ui_tests.rs"]
mod tests;

use crate::{
    DisunityHost, HostError, native,
    presentation::{Coverage, decode_quad, read_coverage},
};
use worldrunner_world_simulation::ui::presentation::{UiDraw, UiDrawKind, UiPresentation};

type Count = unsafe extern "C" fn(u64, *mut u32, *mut u32) -> native::DisunityResult;
type Draw = unsafe extern "C" fn(
    u64,
    u32,
    *mut native::DisunityUiDraw,
    *mut native::DisunityQuad,
) -> native::DisunityResult;

impl DisunityHost {
    /// Reads typed pixel-space UI from the last `export_presentation`, before its lossy live
    /// 2D conversion. Preparation and unload replace/release the snapshot with the scene frame.
    pub fn ui_presentation(&self) -> Result<UiPresentation, HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        let count: Count = self.ui_symbol(b"disunity_presentation_ui_count\0")?;
        let draw: Draw = self.ui_symbol(b"disunity_presentation_ui_draw\0")?;
        let coverage: Coverage = self.ui_symbol(b"disunity_presentation_ui_coverage\0")?;
        let (mut draws, mut textures) = (0, 0);
        self.check(unsafe { count(world, &mut draws, &mut textures) })?;
        let mut plan = UiPresentation::default();
        for index in 0..draws {
            let mut meta = native::DisunityUiDraw::default();
            let mut quad = native::DisunityQuad::default();
            self.check(unsafe { draw(world, index, &mut meta, &mut quad) })?;
            plan.draws.push(decode(meta, quad)?);
        }
        plan.coverage =
            read_coverage(world, textures, coverage).map_err(|failed| self.native_error(failed))?;
        Ok(plan)
    }

    fn ui_symbol<T: Copy>(&self, name: &'static [u8]) -> Result<T, HostError> {
        unsafe { self._library.get(name).map(|symbol| *symbol) }.map_err(|_| {
            HostError::MissingSymbol(String::from_utf8_lossy(name).trim_end_matches('\0').into())
        })
    }
}

fn decode(meta: native::DisunityUiDraw, quad: native::DisunityQuad) -> Result<UiDraw, HostError> {
    let kind = match meta.kind & !4 {
        0 | 2 => UiDrawKind::Graphic,
        1 => UiDrawKind::Text,
        kind => return Err(HostError::Managed(format!("unknown UI draw kind {kind}"))),
    };
    if quad.overlay != 1 {
        return Err(HostError::Managed(
            "UI overlay metadata on a non-overlay quad".into(),
        ));
    }
    let corner_colors = decode_corner_colors(&meta)?;
    let quad = decode_quad(quad);
    Ok(UiDraw {
        kind,
        depth_test: meta.kind & 4 != 0,
        corner_colors,
        triangle_uv: (meta.kind & !4 == 2).then_some([
            [meta.triangle_u0, meta.triangle_v0],
            [meta.triangle_u1, meta.triangle_v1],
            [meta.triangle_u2, meta.triangle_v2],
        ]),
        corners: quad.corners,
        uv_rect: quad.uv_rect,
        color: quad.color,
        texture: quad.texture,
        sorting_layer: quad.sorting_layer,
        sorting_order: quad.sorting_order,
        hierarchy_order: usize::try_from(meta.hierarchy_order)
            .map_err(|_| HostError::Managed("UI hierarchy order overflow".into()))?,
        clip_rect: [meta.clip_x, meta.clip_y, meta.clip_width, meta.clip_height],
    })
}

fn decode_corner_colors(meta: &native::DisunityUiDraw) -> Result<Option<[[f32; 4]; 4]>, HostError> {
    let colors = [
        [
            meta.corner_0_r,
            meta.corner_0_g,
            meta.corner_0_b,
            meta.corner_0_a,
        ],
        [
            meta.corner_1_r,
            meta.corner_1_g,
            meta.corner_1_b,
            meta.corner_1_a,
        ],
        [
            meta.corner_2_r,
            meta.corner_2_g,
            meta.corner_2_b,
            meta.corner_2_a,
        ],
        [
            meta.corner_3_r,
            meta.corner_3_g,
            meta.corner_3_b,
            meta.corner_3_a,
        ],
    ];
    if meta.corner_colors_present > 1
        || colors
            .into_iter()
            .flatten()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(&v))
    {
        return Err(HostError::Managed("invalid UI corner_colors".into()));
    }
    Ok((meta.corner_colors_present == 1).then_some(colors))
}
