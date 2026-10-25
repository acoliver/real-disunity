//! Pathless CPU UI contributions for the existing scene compositor.
//! Texture coverage is linear R8; a glyph quad samples it as alpha. Overlay
//! vertices are pixels, other vertices use the native world basis. Rendering
//! and GPU resource lifetimes remain the scene renderer's responsibility.

use crate::presentation::CoverageTexture as SceneTexture;
use crate::ui::{CanvasMode, FontBook, RectLayout, TextLayout, UiError, UiLayout};
use crate::{ComponentDesc, ObjectHandle, World, WorldError};

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
    let layout = world.ui_layout(viewport.into())?;
    let snapshot = world.snapshot();
    let mut texts = Vec::new();
    let mut budget = FrameBudget::default();
    for component in &snapshot.components {
        let ComponentDesc::Text(run) = &component.desc else {
            continue;
        };
        if !run.enabled {
            continue;
        }
        let Some(rect) = layout.rect(component.object) else {
            validate_hidden_text(world, component.object, &snapshot.components)?;
            continue;
        };
        let text = fonts.layout(run, rect.size)?;
        budget.add(
            run.text.chars().filter(|c| *c != '\r').count(),
            text.atlas.alpha.len(),
            fonts.limits(),
        )?;
        let overlay = snapshot.components.iter().any(|c| c.object == rect.canvas && matches!(c.desc, ComponentDesc::Canvas(canvas) if canvas.mode == CanvasMode::ScreenSpaceOverlay));
        texts.push(contribution(rect, text, overlay, run.color));
    }
    texts.sort_by_key(|t| (t.sorting_layer, t.sorting_order, t.object));
    Ok(UiRenderPlan { layout, texts })
}

fn contribution(
    rect: &RectLayout,
    text: TextLayout,
    overlay: bool,
    color: [f32; 4],
) -> UiTextContribution {
    let atlas_size = text.atlas.size.as_vec2();
    let mut quads = Vec::new();
    for glyph in text.glyphs {
        let min = glyph.rect_min - rect.pivot * rect.size;
        let max = glyph.rect_max - rect.pivot * rect.size;
        let positions = [min, [max.x, min.y].into(), max, [min.x, max.y].into()];
        let corners = positions.map(|p| {
            rect.local_to_world
                .transform_point3(p.extend(0.))
                .to_array()
        });
        let uv_min = glyph.atlas_min.as_vec2() / atlas_size;
        let uv_max = (glyph.atlas_min + glyph.atlas_size).as_vec2() / atlas_size;
        let uv = [
            [uv_min.x, uv_max.y],
            [uv_max.x, uv_max.y],
            [uv_max.x, uv_min.y],
            [uv_min.x, uv_min.y],
        ];
        quads.push(UiGlyphQuad { corners, uv });
    }
    let texture = if text.atlas.alpha.is_empty() {
        None
    } else {
        Some(SceneTexture {
            width: text.atlas.size.x,
            height: text.atlas.size.y,
            bytes: text.atlas.alpha.into_boxed_slice(),
        })
    };
    UiTextContribution {
        object: rect.object,
        canvas: rect.canvas,
        overlay,
        sorting_layer: rect.sorting_layer,
        sorting_order: rect.sorting_order,
        color,
        texture,
        quads,
    }
}

fn validate_hidden_text(
    world: &World,
    object: ObjectHandle,
    components: &[crate::ComponentSnapshot],
) -> Result<(), WorldError> {
    if !world.active_in_hierarchy(object)? {
        return Ok(());
    }
    if !components
        .iter()
        .any(|c| c.object == object && matches!(c.desc, ComponentDesc::RectTransform(_)))
    {
        return Err(UiError::MissingLayout.into());
    }
    let mut current = Some(object);
    while let Some(node) = current {
        if components.iter().any(|c| {
            c.object == node && matches!(c.desc, ComponentDesc::Canvas(canvas) if !canvas.enabled)
        }) {
            return Ok(());
        }
        current = world.parent(node)?;
    }
    Err(UiError::MissingLayout.into())
}

#[derive(Default)]
struct FrameBudget {
    glyphs: usize,
    pixels: usize,
}
impl FrameBudget {
    fn add(
        &mut self,
        glyphs: usize,
        pixels: usize,
        limits: crate::ui::UiLimits,
    ) -> Result<(), UiError> {
        self.glyphs = self.glyphs.checked_add(glyphs).ok_or(UiError::GlyphLimit)?;
        self.pixels = self.pixels.checked_add(pixels).ok_or(UiError::AtlasLimit)?;
        if self.glyphs > limits.max_glyphs {
            return Err(UiError::GlyphLimit);
        }
        if self.pixels > limits.max_atlas_pixels {
            return Err(UiError::AtlasLimit);
        }
        Ok(())
    }
}
