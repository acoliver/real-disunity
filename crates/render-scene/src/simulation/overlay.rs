//! Screen-space native text and graphics for the retained scene compositor.
use crate::dynamic::{DynamicError, DynamicQuad};
use std::collections::BTreeMap;
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace,
    ui::{UiDraw, UiDrawKind, UiPresentation, UiRenderPlan},
};

/// Validated, pathless lower-left pixel geometry. World canvases use the scene path.
#[derive(Clone, Debug)]
pub struct UiOverlay {
    pub(crate) textures: Vec<SceneTexture>,
    pub(crate) quads: Vec<DynamicQuad>,
}
impl UiOverlay {
    pub fn needs_depth(&self) -> bool {
        self.quads.iter().any(|quad| quad.depth_test)
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
        if vertices > worldrunner_render_plan::ui::annotations::MAX_ANNOTATION_VERTICES {
            return Err(DynamicError::DeviceLimit("UI frame vertex budget (65536)"));
        }
        let mut result = Self {
            textures: vec![],
            quads: vec![],
        };
        let mut indices = BTreeMap::new();
        for draw in &plan.draws {
            let key = draw.texture.map(|id| (draw.kind, id));
            let texture = match key {
                Some(key) => Some(if let Some(&index) = indices.get(&key) {
                    index
                } else {
                    let source = draw_texture(draw, plan, textures, result.quads.len())?;
                    let index = result.textures.len();
                    result.textures.push(source);
                    indices.insert(key, index);
                    index
                }),
                None if draw.kind == UiDrawKind::Text => {
                    return Err(DynamicError::InvalidQuad {
                        index: result.quads.len(),
                        field: "glyph coverage texture",
                    });
                }
                None => None,
            };
            result.quads.push(dynamic_quad(draw, texture));
        }
        for (index, quad) in result.quads.iter().enumerate() {
            crate::dynamic::validation::validate_quad(quad, index, result.textures.len())?;
        }
        Ok(result)
    }
}

fn dynamic_quad(draw: &UiDraw, texture: Option<usize>) -> DynamicQuad {
    DynamicQuad {
        overlay: true,
        depth_test: draw.depth_test,
        triangle_uv: draw.triangle_uv,
        corners: Some(draw.corners),
        uv_rect: draw.uv_rect,
        tint: draw.color,
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
    let identity = draw.texture.expect("textured draw");
    if draw.kind == UiDrawKind::Graphic {
        return textures
            .get(&identity)
            .cloned()
            .ok_or_else(|| invalid("unresolved UI texture identity"));
    }
    let texture = plan
        .coverage
        .get(&identity)
        .ok_or_else(|| invalid("glyph coverage texture"))?;
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
