//! Pathless CPU UI contributions for the existing scene compositor.
//! Texture coverage is linear R8; a glyph quad samples it as alpha. Overlay
//! vertices are pixels, other vertices use the native world basis. Rendering
//! and GPU resource lifetimes remain the scene renderer's responsibility.

use crate::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_world_simulation::ui::{FontBook, UiLayout};
use worldrunner_world_simulation::{ObjectHandle, World, WorldError};

#[derive(Clone, Debug, PartialEq)]
pub struct UiGlyphQuad {
    /// Bottom-left, bottom-right, top-right, top-left.
    pub corners: [[f32; 3]; 4],
    /// Normalized UVs in matching order; atlas rows start at the top.
    pub uv: [[f32; 2]; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiTextContribution {
    pub object: ObjectHandle,
    pub canvas: ObjectHandle,
    pub overlay: bool,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub color: [f32; 4],
    pub texture: Option<SceneTexture>,
    pub quads: Vec<UiGlyphQuad>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderPlan {
    pub layout: UiLayout,
    /// Stable order by Canvas sort keys, then object identity.
    pub texts: Vec<UiTextContribution>,
}

pub fn extract_ui(
    world: &World,
    fonts: &FontBook,
    viewport: [u32; 2],
) -> Result<UiRenderPlan, WorldError> {
    let native =
        worldrunner_world_simulation::ui::contribution::extract_ui(world, fonts, viewport)?;
    Ok(UiRenderPlan {
        layout: native.layout,
        texts: native
            .texts
            .into_iter()
            .map(|t| UiTextContribution {
                object: t.object,
                canvas: t.canvas,
                overlay: t.overlay,
                sorting_layer: t.sorting_layer,
                sorting_order: t.sorting_order,
                color: t.color,
                texture: t.texture.map(|t| SceneTexture::Texels {
                    format: TexelFormat::R8Unorm,
                    color_space: TextureColorSpace::Linear,
                    width: t.width,
                    height: t.height,
                    mips: 1,
                    bytes: t.bytes,
                }),
                quads: t
                    .quads
                    .into_iter()
                    .map(|q| UiGlyphQuad {
                        corners: q.corners,
                        uv: q.uv,
                    })
                    .collect(),
            })
            .collect(),
    })
}

pub(crate) fn into_native(
    ui: UiRenderPlan,
) -> Result<
    worldrunner_world_simulation::ui::contribution::UiRenderPlan,
    worldrunner_camera::ViewError,
> {
    use worldrunner_world_simulation::{presentation::CoverageTexture, ui::contribution as n};
    let texts = ui
        .texts
        .into_iter()
        .map(|t| {
            let texture = t
                .texture
                .map(|texture| match texture {
                    SceneTexture::Texels {
                        format: TexelFormat::R8Unorm,
                        width,
                        height,
                        bytes,
                        ..
                    } => Ok(CoverageTexture {
                        width,
                        height,
                        bytes,
                    }),
                    _ => Err(worldrunner_camera::ViewError::InvalidWorldPoint),
                })
                .transpose()?;
            Ok(n::UiTextContribution {
                object: t.object,
                canvas: t.canvas,
                overlay: t.overlay,
                sorting_layer: t.sorting_layer,
                sorting_order: t.sorting_order,
                color: t.color,
                texture,
                quads: t
                    .quads
                    .into_iter()
                    .map(|q| n::UiGlyphQuad {
                        corners: q.corners,
                        uv: q.uv,
                    })
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>, worldrunner_camera::ViewError>>()?;
    Ok(n::UiRenderPlan {
        layout: ui.layout,
        texts,
    })
}
