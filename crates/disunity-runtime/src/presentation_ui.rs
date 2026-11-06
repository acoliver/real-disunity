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
    let kind = match meta.kind {
        0 | 2 => UiDrawKind::Graphic,
        1 => UiDrawKind::Text,
        kind => return Err(HostError::Managed(format!("unknown UI draw kind {kind}"))),
    };
    let quad = decode_quad(quad);
    Ok(UiDraw {
        kind,
        triangle_uv: (meta.kind == 2).then_some([
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triangle_copy_out_preserves_uvs_and_three_corner_topology() {
        let meta = native::DisunityUiDraw {
            kind: 2,
            triangle_u0: 0.1,
            triangle_v0: 0.2,
            triangle_u1: 0.3,
            triangle_v1: 0.4,
            triangle_u2: 0.5,
            triangle_v2: 0.6,
            ..native::DisunityUiDraw::default()
        };
        let quad = native::DisunityQuad {
            p0_x: 10.,
            p1_x: 20.,
            p2_y: 30.,
            ..native::DisunityQuad::default()
        };
        let draw = decode(meta, quad).unwrap();
        assert_eq!(draw.kind, UiDrawKind::Graphic);
        assert_eq!(draw.triangle_uv, Some([[0.1, 0.2], [0.3, 0.4], [0.5, 0.6]]));
        assert_eq!(
            draw.corners[..3],
            [[10., 0., 0.], [20., 0., 0.], [0., 30., 0.]]
        );
    }
}
