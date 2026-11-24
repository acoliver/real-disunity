//! Detached screen-space draws from the world's UI extraction, without live object handles.
use super::UiTexture;
use super::contribution::UiRenderPlan;
pub use super::distance::{UiDistanceMode, UiDistanceStyle, validate_distance_mapping};
pub use super::layers::{UiLayers, UiRefused, UiRun, UiSpan};
pub use super::material::UiMaterial;
use crate::presentation::CoverageTexture;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum UiDrawKind {
    Graphic,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiDraw {
    pub kind: UiDrawKind,
    /// Compare corners' non-reversed 0..1 raster depth with the retained scene.
    pub depth_test: bool,
    /// Some for triangle lists: first three corners and per-vertex, top-row-origin UVs.
    pub triangle_uv: Option<[[f32; 2]; 3]>,
    /// Lower-left viewport pixels, in bottom-left, bottom-right, top-right, top-left order.
    pub corners: [[f32; 3]; 4],
    pub uv_rect: [f32; 4],
    pub color: [f32; 4],
    /// None samples coverage unchanged; Some interprets the shared atlas as signed distance.
    pub distance_field: Option<UiDistanceStyle>,
    /// Optional linear straight-alpha multipliers in BL/BR/TR/TL order; None is white.
    /// Triangles use the first three colours. Every channel must be finite and in 0..=1.
    pub corner_colors: Option<[[f32; 4]; 4]>,
    /// A graphic's image or render texture, or a key in `UiPresentation::coverage` for text.
    pub texture: Option<UiTexture>,
    /// The game's own material of a graphic, or none for the engine's look and for all text.
    pub material: Option<UiMaterial>,
    pub sorting_layer: i32,
    pub sorting_order: i32,
    pub hierarchy_order: usize,
    pub clip_rect: [u32; 4],
}

impl UiDraw {
    /// A text draw names coverage and a graphic draw an image or render texture. A distance field
    /// needs a text draw over coverage without triangle UVs.
    pub fn validate(&self) -> Result<(), super::UiError> {
        let paired = matches!(
            (self.kind, self.texture),
            (_, None)
                | (UiDrawKind::Text, Some(UiTexture::Coverage(_)))
                | (
                    UiDrawKind::Graphic,
                    Some(UiTexture::Image(_) | UiTexture::Target(_))
                )
        );
        if !paired {
            return Err(super::UiError::InvalidTextEffect);
        }
        if let Some(style) = self.distance_field {
            style.validate()?;
            let coverage = matches!(self.texture, Some(UiTexture::Coverage(_)));
            if self.kind != UiDrawKind::Text || !coverage || self.triangle_uv.is_some() {
                return Err(super::UiError::InvalidTextEffect);
            }
            validate_distance_mapping(self.corners, self.uv_rect)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UiPresentation {
    pub draws: Vec<UiDraw>,
    pub coverage: BTreeMap<u64, CoverageTexture>,
    /// The shader name of each material a draw names. The ABI carries material identities only,
    /// so the host that reads the draws looks the names up; a plan built in process has none.
    pub shaders: BTreeMap<UiMaterial, String>,
}

impl UiPresentation {
    /// Adds a detached contribution, remapping text coverage identities without changing
    /// graphic image identities. Vertex limits apply to the combined frame, atomically.
    pub fn append(&mut self, mut other: Self) -> Result<(), super::UiError> {
        let vertices: usize = self
            .draws
            .iter()
            .chain(&other.draws)
            .map(|d| if d.triangle_uv.is_some() { 3 } else { 6 })
            .sum();
        if vertices > super::annotations::MAX_ANNOTATION_VERTICES {
            return Err(super::UiError::AnnotationLimit);
        }
        let offset = if other.coverage.is_empty() {
            0
        } else {
            self.coverage
                .last_key_value()
                .map_or(Some(0), |(id, _)| id.checked_add(1))
                .ok_or(super::UiError::AtlasLimit)?
        };
        if other
            .coverage
            .keys()
            .any(|id| id.checked_add(offset).is_none())
        {
            return Err(super::UiError::AtlasLimit);
        }
        for draw in &mut other.draws {
            if draw.kind == UiDrawKind::Text {
                draw.texture = draw
                    .texture
                    .map(|texture| match texture {
                        UiTexture::Coverage(id) => id
                            .checked_add(offset)
                            .map(UiTexture::Coverage)
                            .ok_or(super::UiError::AtlasLimit),
                        other => Ok(other),
                    })
                    .transpose()?;
            }
        }
        self.coverage.extend(
            other
                .coverage
                .into_iter()
                .map(|(id, texture)| (id + offset, texture)),
        );
        self.shaders.extend(other.shaders);
        self.draws.extend(other.draws);
        Ok(())
    }

    /// Preserves pixel coordinates, clipping and `RectLayout::draw_order()` before the live
    /// 2D path converts UI to world-space sprite quads. Non-overlay canvases stay on that path.
    pub fn from_plan(plan: &UiRenderPlan) -> Self {
        let mut result = Self::default();
        for contribution in &plan.graphics {
            let graphic = &contribution.graphic;
            if !graphic.overlay {
                continue;
            }
            for triangle in &graphic.triangles {
                let [a, b, c] = *triangle;
                result.draws.push(UiDraw {
                    kind: UiDrawKind::Graphic,
                    depth_test: false,
                    corner_colors: None,
                    distance_field: None,
                    triangle_uv: Some(triangle.map(|v| [v.uv[0], 1. - v.uv[1]])),
                    corners: [a.position, b.position, c.position, c.position],
                    uv_rect: [0.; 4],
                    color: a.color,
                    texture: graphic.texture,
                    material: graphic.material,
                    sorting_layer: graphic.sorting_layer,
                    sorting_order: graphic.sorting_order,
                    hierarchy_order: contribution.hierarchy_order,
                    clip_rect: contribution.clip_rect,
                });
            }
            for quad in &graphic.quads {
                result.draws.push(UiDraw {
                    kind: UiDrawKind::Graphic,
                    depth_test: false,
                    corner_colors: None,
                    distance_field: None,
                    triangle_uv: None,
                    corners: quad.corners,
                    uv_rect: quad.uv_rect,
                    color: quad.color,
                    texture: graphic.texture,
                    material: graphic.material,
                    sorting_layer: graphic.sorting_layer,
                    sorting_order: graphic.sorting_order,
                    hierarchy_order: contribution.hierarchy_order,
                    clip_rect: contribution.clip_rect,
                });
            }
        }
        result.append_text(plan);
        result.draws.sort_by_key(|draw| {
            (
                draw.sorting_layer,
                draw.sorting_order,
                draw.hierarchy_order,
                draw.kind,
            )
        });
        result
    }

    fn append_text(&mut self, plan: &UiRenderPlan) {
        for (index, text) in plan.texts.iter().enumerate().filter(|(_, t)| t.overlay) {
            let identity = index as u64;
            if let Some(texture) = &text.texture {
                self.coverage.insert(identity, texture.clone());
            }
            for glyph in &text.quads {
                let uv = glyph.uv;
                self.draws.push(UiDraw {
                    kind: UiDrawKind::Text,
                    depth_test: false,
                    corner_colors: None,
                    distance_field: None,
                    triangle_uv: None,
                    corners: glyph.corners,
                    uv_rect: [uv[0][0], uv[2][1], uv[2][0], uv[0][1]],
                    color: std::array::from_fn(|i| text.color[i] * glyph.color[i]),
                    texture: text.texture.as_ref().map(|_| UiTexture::Coverage(identity)),
                    material: None,
                    sorting_layer: text.sorting_layer,
                    sorting_order: text.sorting_order,
                    hierarchy_order: text.hierarchy_order,
                    clip_rect: text.clip_rect,
                });
            }
        }
    }
}
