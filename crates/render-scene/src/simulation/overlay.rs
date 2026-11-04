//! Screen-space native text for the retained scene compositor.
use crate::dynamic::{DynamicError, DynamicQuad};
use worldrunner_render_plan::{
    SceneTexture, TexelFormat, TextureColorSpace,
    ui::{UiRenderPlan, UiTextContribution},
};

/// Validated, pathless lower-left pixel geometry. World canvases use the scene path.
#[derive(Clone, Debug)]
pub struct UiOverlay {
    pub(crate) textures: Vec<SceneTexture>,
    pub(crate) quads: Vec<DynamicQuad>,
}
impl UiOverlay {
    pub fn new(plan: UiRenderPlan) -> Result<Self, DynamicError> {
        let mut result = Self {
            textures: vec![],
            quads: vec![],
        };
        for (index, text) in plan.texts.into_iter().enumerate() {
            result.append(index, text)?;
        }
        for (index, quad) in result.quads.iter().enumerate() {
            crate::dynamic::validation::validate_quad(quad, index, result.textures.len())?;
        }
        Ok(result)
    }

    fn append(&mut self, index: usize, text: UiTextContribution) -> Result<(), DynamicError> {
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
        self.textures.push(
            SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space: TextureColorSpace::Linear,
                width: texture.width,
                height: texture.height,
                mips: 1,
                bytes: texture
                    .bytes
                    .iter()
                    .flat_map(|&alpha| [255, 255, 255, alpha])
                    .collect(),
            }
            .with_sampler(worldrunner_render_plan::TextureSampler {
                filter: worldrunner_render_plan::TextureFilter::Bilinear,
                wrap_u: worldrunner_render_plan::TextureWrap::Clamp,
                wrap_v: worldrunner_render_plan::TextureWrap::Clamp,
            }),
        );
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
                ..DynamicQuad::default()
            });
        }
        Ok(())
    }
}
