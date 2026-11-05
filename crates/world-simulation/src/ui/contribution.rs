//! Pathless CPU UI contributions for the existing scene compositor.
//! Texture coverage is linear R8; a glyph quad samples it as alpha. Overlay
//! vertices are pixels, other vertices use the native world basis. Rendering
//! and GPU resource lifetimes remain the scene renderer's responsibility.

mod clip;

use crate::presentation::CoverageTexture as SceneTexture;
use crate::presentation::extraction::{camera_admits, selected_camera};
use crate::ui::group::{effective_alpha, groups_by_object};
use crate::ui::{Canvas, CanvasMode, FontBook, RectLayout, TextLayout, UiError, UiLayout};
use crate::{ComponentDesc, ObjectHandle, World, WorldError};
use glam::{Vec2, Vec3};

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
    /// The rectangle hierarchy key from `RectLayout::draw_order()`.
    pub hierarchy_order: usize,
    /// Lower-left pixel clip bounds: x, y, width, height.
    pub clip_rect: [u32; 4],
    pub color: [f32; 4],
    pub texture: Option<SceneTexture>,
    pub quads: Vec<UiGlyphQuad>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiGraphicContribution {
    pub graphic: crate::ui::UiGraphic,
    pub hierarchy_order: usize,
    /// Lower-left pixel clip bounds: x, y, width, height.
    pub clip_rect: [u32; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiRenderPlan {
    pub layout: UiLayout,
    /// Ascending `RectLayout::draw_order()`, shared with pointer hit testing.
    pub texts: Vec<UiTextContribution>,
    pub graphics: Vec<UiGraphicContribution>,
}

pub fn extract_ui(
    world: &World,
    fonts: &FontBook,
    viewport: [u32; 2],
) -> Result<UiRenderPlan, WorldError> {
    extract_ui_layout(world, fonts, world.ui_layout(viewport.into())?)
}

/// Extract a freshly resolved layout, including caller-owned menu rules and scroll clips.
pub fn extract_ui_layout(
    world: &World,
    fonts: &FontBook,
    layout: UiLayout,
) -> Result<UiRenderPlan, WorldError> {
    let roots = super::graphics::root_canvases(world)?;
    let mut texts = Vec::new();
    let mut budget = FrameBudget::default();
    let groups = groups_by_object(world.published_components());
    let camera = selected_camera(world, world.published_components())?.map(|(_, camera)| camera);
    for component in world.published_components() {
        let ComponentDesc::Text(run) = &component.desc else {
            continue;
        };
        if !run.enabled {
            continue;
        }
        let Some(rect) = layout.rect(component.object) else {
            validate_hidden_text(world, component.object)?;
            continue;
        };
        let canvas = world
            .published_components()
            .find_map(|c| match c.desc {
                ComponentDesc::Canvas(canvas) if c.object == rect.canvas => Some(canvas),
                _ => None,
            })
            .unwrap_or_default();
        let overlay = rect.mode == CanvasMode::ScreenSpaceOverlay;
        if !overlay && !camera_admits(world, camera, component.object)? {
            continue;
        }
        let text = fonts.layout(run, rect.size.max(Vec2::ZERO))?;
        budget.add(
            run.text.chars().filter(|c| *c != '\r').count(),
            text.atlas.alpha.len(),
            fonts.limits(),
        )?;
        let mut color = run.color;
        color[3] *= effective_alpha(world, &groups, component.object)?;
        let mut text = contribution(rect, text, overlay, color);
        text.clip_rect = clip::screen_clip(world, &layout, rect, &roots);
        snap_to_pixels(&mut text, rect, canvas);
        texts.push(text);
    }
    texts.sort_by_key(|t| (t.sorting_layer, t.sorting_order, t.hierarchy_order));
    let graphics = graphics(world, &layout, &roots)?;
    Ok(UiRenderPlan {
        layout,
        texts,
        graphics,
    })
}

fn contribution(
    rect: &RectLayout,
    text: TextLayout,
    overlay: bool,
    color: [f32; 4],
) -> UiTextContribution {
    let (sorting_layer, sorting_order, hierarchy_order) = rect.draw_order();
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
        sorting_layer,
        sorting_order,
        hierarchy_order,
        clip_rect: [0; 4],
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

fn validate_hidden_text(world: &World, object: ObjectHandle) -> Result<(), WorldError> {
    if !world.active_in_hierarchy(object)? {
        return Ok(());
    }
    if !world
        .published_components()
        .any(|c| c.object == object && matches!(c.desc, ComponentDesc::RectTransform(_)))
    {
        return Err(UiError::MissingLayout.into());
    }
    let mut current = Some(object);
    while let Some(node) = current {
        if world.published_components().any(|c| {
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

fn graphics(
    world: &World,
    layout: &UiLayout,
    roots: &std::collections::BTreeMap<ObjectHandle, ObjectHandle>,
) -> Result<Vec<UiGraphicContribution>, WorldError> {
    let mut graphics = crate::ui::extract_graphics(world, layout)?
        .into_iter()
        .map(|graphic| {
            let rect = layout
                .rect(graphic.object)
                .expect("extracted graphic rectangle");
            Ok(UiGraphicContribution {
                hierarchy_order: rect.draw_order().2,
                clip_rect: clip::screen_clip(world, layout, rect, roots),
                graphic,
            })
        })
        .collect::<Result<Vec<_>, WorldError>>()?;
    graphics.sort_by_key(|c| {
        (
            c.graphic.sorting_layer,
            c.graphic.sorting_order,
            c.hierarchy_order,
        )
    });
    Ok(graphics)
}
