//! Camera projection for the normalized rectangular material quadtree.
use super::selection::{PageSelection, select_pages};
use glam::{Mat4, Vec2, Vec3};
use worldrunner_render_plan::TerrainPageSettings;

pub struct PageView {
    pub matrix: Mat4,
    pub viewport: [u32; 2],
    pub origin: [f32; 2],
    pub extent: [f32; 2],
    pub elevation: [f32; 2],
}

pub fn select(
    settings: TerrainPageSettings,
    previous: Option<&PageSelection>,
    matrix: Mat4,
    viewport: [u32; 2],
    origin: [f32; 2],
    extent: [f32; 2],
) -> PageSelection {
    select_volume(
        settings,
        previous,
        &PageView {
            matrix,
            viewport,
            origin,
            extent,
            elevation: [0.0; 2],
        },
    )
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bounded projected pixel error is saturated into the selection domain"
)]
pub fn select_volume(
    settings: TerrainPageSettings,
    previous: Option<&PageSelection>,
    view: &PageView,
) -> PageSelection {
    let pixels = Vec2::new(view.viewport[0] as f32, view.viewport[1] as f32) * 0.5;
    select_pages(settings, previous, |key| {
        let scale = (1_u32 << key.depth()) as f32;
        let coordinate = key.coordinate();
        let minimum = std::array::from_fn::<_, 2, _>(|axis| {
            view.origin[axis] + view.extent[axis] * coordinate[axis] as f32 / scale
        });
        let mut error = 0.0_f32;
        let prism = view.elevation.map(|elevation| {
            [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]].map(|uv| {
                view.matrix
                    * Vec3::new(
                        minimum[0] + uv[0] * view.extent[0] / scale,
                        minimum[1] + uv[1] * view.extent[1] / scale,
                        elevation,
                    )
                    .extend(1.0)
            })
        });
        if prism.iter().flatten().all(|point| point.w <= 0.0) {
            return 0;
        }
        if prism.iter().flatten().any(|point| point.w <= 0.0) {
            return u32::MAX;
        }
        for corners in prism {
            let projected = corners.map(|point| point.truncate().truncate() / point.w * pixels);
            for (a, b, axis) in [(0, 1, 0), (2, 3, 0), (0, 2, 1), (1, 3, 1)] {
                error = error.max(
                    (projected[b] - projected[a]).length() / settings.parts().interior[axis] as f32,
                );
            }
        }
        (error * 1000.0).min(u32::MAX as f32) as u32
    })
}
