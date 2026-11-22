//! Screen-space native text and graphics for the retained scene compositor.
use crate::dynamic::{DynamicError, DynamicQuad};
use std::collections::BTreeMap;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_world_simulation::ui::{
    contribution::UiRenderPlan,
    presentation::{UiDraw, UiDrawKind, UiLayers, UiPresentation, UiRun},
};

/// Validated, pathless lower-left pixel geometry. World canvases use the scene path.
#[derive(Clone, Debug)]
pub struct UiOverlay {
    pub(crate) textures: Vec<SceneTexture>,
    pub(crate) quads: Vec<DynamicQuad>,
    /// The entry of `textures` that stands for the layer atlas a host offers, when the overlay
    /// has runs to show. The stand-in is transparent until the viewer binds the atlas to it.
    pub(crate) layer: Option<usize>,
}
impl UiOverlay {
    pub fn needs_depth(&self) -> bool {
        self.quads.iter().any(|quad| quad.depth_test)
    }

    /// How many quads the overlay draws: one for each default draw and one for each run.
    pub fn quad_count(&self) -> usize {
        self.quads.len()
    }

    /// Whether the overlay shows runs of the layer atlas, which the host offers separately.
    pub fn shows_layers(&self) -> bool {
        self.layer.is_some()
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
        Self::from_presentation(&presentation, textures, &UiLayers::default())
    }

    /// Uses the same detached draws for native worlds and prepared player presentations.
    ///
    /// A draw with a game material is drawn by the player's program into the layer atlas, so it
    /// has no quad of its own. Each run of `layers` becomes one quad of the atlas at the sort
    /// position of the run's first draw, and every other draw with a material is left out.
    pub fn from_presentation(
        plan: &UiPresentation,
        textures: &BTreeMap<u64, SceneTexture>,
        layers: &UiLayers,
    ) -> Result<Self, DynamicError> {
        let vertices: usize = plan
            .draws
            .iter()
            .filter(|d| d.material.is_none())
            .map(|d| if d.triangle_uv.is_some() { 3 } else { 6 })
            .sum::<usize>()
            + layers.runs.len() * 6;
        if vertices > worldrunner_world_simulation::ui::annotations::MAX_ANNOTATION_VERTICES {
            return Err(DynamicError::DeviceLimit("UI frame vertex budget (65536)"));
        }
        let mut builder = Builder {
            plan,
            sources: textures,
            indices: BTreeMap::new(),
            result: Self {
                textures: vec![],
                quads: vec![],
                layer: None,
            },
        };
        let mut runs = layers.runs.iter().peekable();
        for (position, draw) in plan.draws.iter().enumerate() {
            if draw.material.is_none() {
                builder.draw(draw)?;
            } else if let Some(run) = runs.next_if(|run| run.draws.start == position) {
                builder.run(run, draw, layers.atlas_size);
            }
        }
        let result = builder.result;
        for (index, quad) in result.quads.iter().enumerate() {
            crate::dynamic::validation::validate_quad(quad, index, result.textures.len())?;
        }
        Ok(result)
    }
}

/// An overlay under construction: the texture table entries made so far, by the draw texture
/// they came from.
struct Builder<'a> {
    plan: &'a UiPresentation,
    sources: &'a BTreeMap<u64, SceneTexture>,
    indices: BTreeMap<(UiDrawKind, u64), usize>,
    result: UiOverlay,
}

impl Builder<'_> {
    /// Adds the quad that shows `run`, making the atlas's stand-in texture the first time.
    fn run(&mut self, run: &UiRun, first: &UiDraw, atlas: [u32; 2]) {
        let result = &mut self.result;
        let slot = *result.layer.get_or_insert_with(|| {
            result.textures.push(layer_stand_in());
            result.textures.len() - 1
        });
        result.quads.push(layer_quad(run, first, atlas, slot));
    }

    fn draw(&mut self, draw: &UiDraw) -> Result<(), DynamicError> {
        let index = self.result.quads.len();
        let invalid = |field| DynamicError::InvalidQuad { index, field };
        draw.validate_distance()
            .map_err(|_| invalid("distance_field"))?;
        if let Some(style) = draw.distance_field {
            let atlas = self
                .plan
                .coverage
                .get(&draw.texture.expect("validated distance texture"))
                .ok_or(invalid("distance atlas"))?;
            style
                .validate_extent(
                    draw.corners,
                    draw.uv_rect,
                    glam::UVec2::new(atlas.width, atlas.height),
                )
                .map_err(|_| invalid("distance field spread"))?;
        }
        let texture = self.texture(draw)?;
        self.result.quads.push(dynamic_quad(draw, texture));
        Ok(())
    }

    /// The texture table entry of the draw's texture, adding it the first time a draw names it.
    fn texture(&mut self, draw: &UiDraw) -> Result<Option<usize>, DynamicError> {
        let index = self.result.quads.len();
        let Some(identity) = draw.texture else {
            return if draw.kind == UiDrawKind::Text {
                Err(DynamicError::InvalidQuad {
                    index,
                    field: "glyph coverage texture",
                })
            } else {
                Ok(None)
            };
        };
        let key = (draw.kind, identity);
        if let Some(&held) = self.indices.get(&key) {
            return Ok(Some(held));
        }
        let source = draw_texture(draw, self.plan, self.sources, index)?;
        let held = self.result.textures.len();
        self.result.textures.push(source);
        self.indices.insert(key, held);
        Ok(Some(held))
    }
}

/// The quad that shows `run` of the atlas where the run's first draw sorts. The atlas rows are
/// bottom first, so the run's top edge reads the larger texture coordinate.
fn layer_quad(run: &UiRun, first: &UiDraw, atlas: [u32; 2], texture: usize) -> DynamicQuad {
    let [x, y, width, height] = glam::UVec4::from(run.bounds).as_vec4().to_array();
    let [ax, ay, aw, ah] = glam::UVec4::from(run.atlas).as_vec4().to_array();
    let [atlas_width, atlas_height] = glam::UVec2::from(atlas).as_vec2().to_array();
    DynamicQuad {
        overlay: true,
        corners: Some([
            [x, y, 0.],
            [x + width, y, 0.],
            [x + width, y + height, 0.],
            [x, y + height, 0.],
        ]),
        uv_rect: [
            ax / atlas_width,
            (ay + ah) / atlas_height,
            (ax + aw) / atlas_width,
            ay / atlas_height,
        ],
        texture: Some(texture),
        sorting_layer: first.sorting_layer,
        sorting_order: first.sorting_order,
        hierarchy_order: first.hierarchy_order,
        ..DynamicQuad::default()
    }
}

/// What the layer atlas is until a host offers it: one transparent texel.
fn layer_stand_in() -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0; 4].into_boxed_slice(),
    }
    .with_sampler(worldrunner_render_plan::TextureSampler {
        filter: worldrunner_render_plan::TextureFilter::Point,
        wrap_u: worldrunner_render_plan::TextureWrap::Clamp,
        wrap_v: worldrunner_render_plan::TextureWrap::Clamp,
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

#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;
