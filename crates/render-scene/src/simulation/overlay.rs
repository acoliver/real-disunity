//! Screen-space native text and graphics for the retained scene compositor.
use crate::dynamic::{DynamicError, DynamicQuad};
use std::collections::BTreeMap;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_world_simulation::graphics::RenderTextureId;
use worldrunner_world_simulation::ui::{
    UiTexture,
    contribution::UiRenderPlan,
    presentation::{UiDraw, UiDrawKind, UiLayers, UiPresentation, UiRun},
};

/// The overlay texture each kind of draw and texture got when first named.
type TextureIndices = BTreeMap<(UiDrawKind, UiTexture), usize>;

/// Validated, pathless lower-left pixel geometry. World canvases use the scene path.
#[derive(Clone, Debug)]
pub struct UiOverlay {
    pub(crate) textures: Vec<SceneTexture>,
    pub(crate) quads: Vec<DynamicQuad>,
    /// The render textures the draws read, one slot for each. A quad of a target draw names its
    /// slot after the overlay's own textures: `textures.len() + slot`.
    pub(crate) targets: Vec<RenderTextureId>,
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
        let mut result = Self {
            textures: vec![],
            quads: vec![],
            targets: vec![],
            layer: None,
        };
        let mut indices = BTreeMap::new();
        let mut slotted = Vec::new();
        let mut runs = layers.runs.iter().peekable();
        for (position, draw) in plan.draws.iter().enumerate() {
            if draw.material.is_none() {
                result.add_draw(draw, (plan, textures), (&mut indices, &mut slotted))?;
            } else if let Some(run) = runs.next_if(|run| run.draws.start == position) {
                result.add_run(run, draw, layers.atlas_size);
            }
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

    /// Adds the quad of a draw of the engine's own look. A target draw is slotted after the
    /// overlay's own textures once all of them are known.
    fn add_draw(
        &mut self,
        draw: &UiDraw,
        (plan, textures): (&UiPresentation, &BTreeMap<u64, SceneTexture>),
        (indices, slotted): (&mut TextureIndices, &mut Vec<(usize, usize)>),
    ) -> Result<(), DynamicError> {
        let index = self.quads.len();
        check_distance(draw, plan, index)?;
        let quad = if let Some(UiTexture::Target(id)) = draw.texture {
            slotted.push((index, self.slot(id)));
            flipped(dynamic_quad(draw, None))
        } else {
            let texture = self.texture_index(draw, (plan, textures), indices)?;
            dynamic_quad(draw, texture)
        };
        self.quads.push(quad);
        Ok(())
    }

    /// Adds the quad that shows `run`, making the atlas's stand-in texture the first time.
    fn add_run(&mut self, run: &UiRun, first: &UiDraw, atlas: [u32; 2]) {
        let slot = *self.layer.get_or_insert_with(|| {
            self.textures.push(layer_stand_in());
            self.textures.len() - 1
        });
        self.quads.push(layer_quad(run, first, atlas, slot));
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
        indices: &mut TextureIndices,
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
#[cfg(test)]
#[path = "overlay_tests.rs"]
mod tests;
