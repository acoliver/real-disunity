//! Screen-space native text and graphics for the retained scene compositor.
use crate::dynamic::{DynamicError, DynamicQuad};
use std::collections::BTreeMap;
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace,
    ui::{UiGraphicContribution, UiRenderPlan, UiTextContribution},
};

/// Validated, pathless lower-left pixel geometry. World canvases use the scene path.
#[derive(Clone, Debug)]
pub struct UiOverlay {
    pub(crate) textures: Vec<SceneTexture>,
    pub(crate) quads: Vec<DynamicQuad>,
}
impl UiOverlay {
    /// Builds text and untextured graphics; unresolved image identities are rejected.
    pub fn new(plan: UiRenderPlan) -> Result<Self, DynamicError> {
        Self::with_textures(plan, &BTreeMap::new())
    }

    /// Resolves graphic texture identities using the existing scene texture contract.
    pub fn with_textures(
        plan: UiRenderPlan,
        textures: &BTreeMap<u64, SceneTexture>,
    ) -> Result<Self, DynamicError> {
        let mut result = Self {
            textures: vec![],
            quads: vec![],
        };
        let mut indices = BTreeMap::new();
        for graphic in plan.graphics {
            result.append_graphic(graphic, textures, &mut indices)?;
        }
        for (index, text) in plan.texts.into_iter().enumerate() {
            result.append_text(index, text)?;
        }
        for (index, quad) in result.quads.iter().enumerate() {
            crate::dynamic::validation::validate_quad(quad, index, result.textures.len())?;
        }
        Ok(result)
    }

    fn append_text(&mut self, index: usize, text: UiTextContribution) -> Result<(), DynamicError> {
        let invalid = |field| DynamicError::InvalidQuad { index, field };
        if !text.overlay {
            return Err(invalid("retained UI requires overlay canvas"));
        }
        let Some(texture) = text.texture else {
            return if text.quads.is_empty() {
                Ok(())
            } else {
                Err(invalid("glyph coverage texture"))
            };
        };
        if texture.width == 0
            || texture.height == 0
            || u64::from(texture.width) * u64::from(texture.height) != texture.bytes.len() as u64
        {
            return Err(invalid("glyph coverage dimensions"));
        }
        let texture_index = self.textures.len();
        self.textures.push(coverage_texture(
            texture.width,
            texture.height,
            &texture.bytes,
        ));
        for glyph in text.quads {
            let uv = glyph.uv;
            self.quads.push(DynamicQuad {
                overlay: true,
                corners: Some(glyph.corners),
                uv_rect: [uv[0][0], uv[2][1], uv[2][0], uv[0][1]],
                tint: text.color,
                texture: Some(texture_index),
                sorting_layer: text.sorting_layer,
                sorting_order: text.sorting_order,
                hierarchy_order: text.hierarchy_order,
                clip_rect: Some(text.clip_rect),
                ..DynamicQuad::default()
            });
        }
        Ok(())
    }

    fn append_graphic(
        &mut self,
        contribution: UiGraphicContribution,
        textures: &BTreeMap<u64, SceneTexture>,
        indices: &mut BTreeMap<u64, usize>,
    ) -> Result<(), DynamicError> {
        let graphic = contribution.graphic;
        let invalid = |field| DynamicError::InvalidQuad {
            index: self.quads.len(),
            field,
        };
        if !graphic.overlay {
            return Err(invalid("retained UI requires overlay canvas"));
        }
        let texture = if let Some(identity) = graphic.texture {
            let source = textures
                .get(&identity)
                .ok_or_else(|| invalid("unresolved UI texture identity"))?;
            Some(*indices.entry(identity).or_insert_with(|| {
                let index = self.textures.len();
                self.textures.push(source.clone());
                index
            }))
        } else {
            None
        };
        for quad in graphic.quads {
            self.quads.push(DynamicQuad {
                overlay: true,
                corners: Some(quad.corners),
                uv_rect: quad.uv_rect,
                tint: quad.color,
                texture,
                sorting_layer: graphic.sorting_layer,
                sorting_order: graphic.sorting_order,
                hierarchy_order: contribution.hierarchy_order,
                clip_rect: Some(contribution.clip_rect),
                ..DynamicQuad::default()
            });
        }
        Ok(())
    }
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
