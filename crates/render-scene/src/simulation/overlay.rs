//! Screen-space native text and graphics for the retained scene compositor.
use crate::dynamic::{DynamicError, DynamicQuad};
use std::collections::BTreeMap;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_world_simulation::graphics::RenderTextureId;
use worldrunner_world_simulation::ui::{
    UiTexture,
    contribution::UiRenderPlan,
    presentation::{UiDraw, UiDrawKind, UiPresentation},
};

/// Validated, pathless lower-left pixel geometry. World canvases use the scene path.
#[derive(Clone, Debug)]
pub struct UiOverlay {
    pub(crate) textures: Vec<SceneTexture>,
    pub(crate) quads: Vec<DynamicQuad>,
    /// The render textures the draws read, one slot for each. A quad of a target draw names its
    /// slot after the overlay's own textures: `textures.len() + slot`.
    pub(crate) targets: Vec<RenderTextureId>,
}
impl UiOverlay {
    pub fn needs_depth(&self) -> bool {
        self.quads.iter().any(|quad| quad.depth_test)
    }

    /// The render textures the draws read. A host offers the device texture of each through
    /// [`UiTarget`](super::UiTarget); one it does not offer draws as transparent black.
    pub fn targets(&self) -> &[RenderTextureId] {
        &self.targets
    }

    /// Builds text and untextured graphics; unresolved image identities are rejected.
    pub fn new(plan: UiRenderPlan) -> Result<Self, DynamicError> {
        Self::with_textures(plan, &BTreeMap::new())
    }

    /// Resolves graphic texture identities using the existing scene texture contract.
    pub fn with_textures(
        plan: UiRenderPlan,
        textures: &BTreeMap<u64, SceneTexture>,
    ) -> Result<Self, DynamicError> {
        if plan.texts.iter().any(|t| !t.overlay) || plan.graphics.iter().any(|g| !g.graphic.overlay)
        {
            return Err(DynamicError::InvalidQuad {
                index: 0,
                field: "retained UI requires overlay canvas",
            });
        }
        let presentation = UiPresentation::from_plan(&plan);
        drop(plan);
        Self::from_presentation(&presentation, textures)
    }

    /// Uses the same detached draws for native worlds and prepared player presentations.
    pub fn from_presentation(
        plan: &UiPresentation,
        textures: &BTreeMap<u64, SceneTexture>,
    ) -> Result<Self, DynamicError> {
        let vertices: usize = plan
            .draws
            .iter()
            .map(|d| if d.triangle_uv.is_some() { 3 } else { 6 })
            .sum();
        if vertices > worldrunner_world_simulation::ui::annotations::MAX_ANNOTATION_VERTICES {
            return Err(DynamicError::DeviceLimit("UI frame vertex budget (65536)"));
        }
        let mut result = Self {
            textures: vec![],
            quads: vec![],
            targets: vec![],
        };
        let mut indices = BTreeMap::new();
        let mut slotted = Vec::new();
        for draw in &plan.draws {
            let index = result.quads.len();
            check_distance(draw, plan, index)?;
            let quad = if let Some(UiTexture::Target(id)) = draw.texture {
                slotted.push((index, result.slot(id)));
                flipped(dynamic_quad(draw, None))
            } else {
                let texture = result.texture_index(draw, (plan, textures), &mut indices)?;
                dynamic_quad(draw, texture)
            };
            result.quads.push(quad);
        }
        let first_slot = result.textures.len();
        for (index, slot) in slotted {
            result.quads[index].texture = Some(first_slot + slot);
        }
        for (index, quad) in result.quads.iter().enumerate() {
            crate::dynamic::validation::validate_quad(
                quad,
                index,
                result.textures.len() + result.targets.len(),
            )?;
        }
        Ok(result)
    }

    /// The slot of the render texture `id`, made when no earlier draw read it.
    fn slot(&mut self, id: RenderTextureId) -> usize {
        self.targets
            .iter()
            .position(|held| *held == id)
            .unwrap_or_else(|| {
                self.targets.push(id);
                self.targets.len() - 1
            })
    }

    /// The index in the overlay's own textures of what `draw` samples, adding the texture when no
    /// earlier draw sampled it.
    fn texture_index(
        &mut self,
        draw: &UiDraw,
        (plan, textures): (&UiPresentation, &BTreeMap<u64, SceneTexture>),
        indices: &mut BTreeMap<(UiDrawKind, UiTexture), usize>,
    ) -> Result<Option<usize>, DynamicError> {
        let Some(texture) = draw.texture else {
            if draw.kind == UiDrawKind::Text {
                return Err(DynamicError::InvalidQuad {
                    index: self.quads.len(),
                    field: "glyph coverage texture",
                });
            }
            return Ok(None);
        };
        let key = (draw.kind, texture);
        if let Some(&index) = indices.get(&key) {
            return Ok(Some(index));
        }
        let source = draw_texture(draw, plan, textures, self.quads.len())?;
        let index = self.textures.len();
        self.textures.push(source);
        indices.insert(key, index);
        Ok(Some(index))
    }
}

/// A render texture is bottom row first and the draw's texture rectangle is top row first, so
/// the draw samples it with `v` turned over.
fn flipped(mut quad: DynamicQuad) -> DynamicQuad {
    let [u0, v0, u1, v1] = quad.uv_rect;
    quad.uv_rect = [u0, 1. - v0, u1, 1. - v1];
    quad.triangle_uv = quad
        .triangle_uv
        .map(|corners| corners.map(|[u, v]| [u, 1. - v]));
    quad
}

fn check_distance(draw: &UiDraw, plan: &UiPresentation, index: usize) -> Result<(), DynamicError> {
    draw.validate().map_err(|_| DynamicError::InvalidQuad {
        index,
        field: "distance_field",
    })?;
    let Some(style) = draw.distance_field else {
        return Ok(());
    };
    let Some(UiTexture::Coverage(key)) = draw.texture else {
        unreachable!("a validated distance draw names coverage");
    };
    let atlas = plan.coverage.get(&key).ok_or(DynamicError::InvalidQuad {
        index,
        field: "distance atlas",
    })?;
    style
        .validate_extent(
            draw.corners,
            draw.uv_rect,
            glam::UVec2::new(atlas.width, atlas.height),
        )
        .map_err(|_| DynamicError::InvalidQuad {
            index,
            field: "distance field spread",
        })
}

fn dynamic_quad(draw: &UiDraw, texture: Option<usize>) -> DynamicQuad {
    DynamicQuad {
        overlay: true,
        depth_test: draw.depth_test,
        triangle_uv: draw.triangle_uv,
        corners: Some(draw.corners),
        uv_rect: draw.uv_rect,
        tint: draw.color,
        corner_colors: draw.corner_colors,
        distance_field: draw.distance_field,
        texture,
        sorting_layer: draw.sorting_layer,
        sorting_order: draw.sorting_order,
        hierarchy_order: draw.hierarchy_order,
        clip_rect: Some(draw.clip_rect),
        ..DynamicQuad::default()
    }
}

fn draw_texture(
    draw: &UiDraw,
    plan: &UiPresentation,
    textures: &BTreeMap<u64, SceneTexture>,
    index: usize,
) -> Result<SceneTexture, DynamicError> {
    let invalid = |field| DynamicError::InvalidQuad { index, field };
    let texture = match draw.texture.expect("textured draw") {
        UiTexture::Image(identity) => {
            return textures
                .get(&identity)
                .cloned()
                .ok_or_else(|| invalid("unresolved UI texture identity"));
        }
        UiTexture::Target(_) => return Err(invalid("render texture target")),
        UiTexture::Coverage(identity) => plan
            .coverage
            .get(&identity)
            .ok_or_else(|| invalid("glyph coverage texture"))?,
    };
    if texture.width == 0
        || texture.height == 0
        || u64::from(texture.width) * u64::from(texture.height) != texture.bytes.len() as u64
    {
        return Err(invalid("glyph coverage dimensions"));
    }
    Ok(coverage_texture(
        texture.width,
        texture.height,
        &texture.bytes,
    ))
}

fn coverage_texture(width: u32, height: u32, bytes: &[u8]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width,
        height,
        mips: 1,
        bytes: bytes
            .iter()
            .flat_map(|&alpha| [255, 255, 255, alpha])
            .collect(),
    }
    .with_sampler(worldrunner_render_plan::TextureSampler {
        filter: worldrunner_render_plan::TextureFilter::Bilinear,
        wrap_u: worldrunner_render_plan::TextureWrap::Clamp,
        wrap_v: worldrunner_render_plan::TextureWrap::Clamp,
    })
}

#[cfg(test)]
#[path = "overlay_target_tests.rs"]
mod target_tests;
