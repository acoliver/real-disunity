//! Pathless CPU UI contributions for the existing scene compositor.
//! Texture coverage is linear R8; a glyph quad samples it as alpha. Overlay
//! vertices are pixels, other vertices use the native world basis. Rendering
//! and GPU resource lifetimes remain the scene renderer's responsibility.

use crate::presentation::CoverageTexture as SceneTexture;
use crate::presentation::extraction::{camera_admits, selected_camera};
use crate::ui::{Canvas, CanvasMode, FontBook, RectLayout, TextLayout, UiError, UiLayout};
use crate::{ComponentDesc, ObjectHandle, World, WorldError};
use glam::Vec3;

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
    let camera = selected_camera(world, &snapshot.components)?.map(|(_, camera)| camera);
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
        let canvas = snapshot
            .components
            .iter()
            .find_map(|c| match c.desc {
                ComponentDesc::Canvas(canvas) if c.object == rect.canvas => Some(canvas),
                _ => None,
            })
            .unwrap_or_default();
        let overlay = canvas.mode == CanvasMode::ScreenSpaceOverlay;
        if !overlay && !camera_admits(world, camera, component.object)? {
            continue;
        }
        let text = fonts.layout(run, rect.size)?;
        budget.add(
            run.text.chars().filter(|c| *c != '\r').count(),
            text.atlas.alpha.len(),
            fonts.limits(),
        )?;
        let mut text = contribution(rect, text, overlay, run.color);
        snap_to_pixels(&mut text, rect, canvas);
        texts.push(text);
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

/// Unity's `Text` on a pixel-perfect canvas: the first vertex (the first glyph's top-left
/// corner) moves to the nearest whole screen pixel, and every glyph moves with it, so glyphs
/// keep their subpixel spacing.
fn snap_to_pixels(text: &mut UiTextContribution, rect: &RectLayout, canvas: Canvas) {
    let Some(pixels) = rect.pixels_from_world.filter(|_| canvas.pixel_perfect) else {
        return;
    };
    let Some(first) = text.quads.first().map(|quad| Vec3::from(quad.corners[3])) else {
        return;
    };
    let screen = pixels.transform_point3(first);
    let offset = (screen.round() - screen) * Vec3::new(1., 1., 0.);
    let shift = pixels.inverse().transform_vector3(offset);
    for quad in &mut text.quads {
        for corner in &mut quad.corners {
            *corner = (Vec3::from(*corner) + shift).to_array();
        }
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
