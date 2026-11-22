//! Native world contributions use the resident scene's existing unlit quad pipeline.
mod overlay;
mod ui_target;
pub use overlay::UiOverlay;
pub use ui_target::{UiTarget, UiTargetRefusal};

use crate::{
    SceneView,
    dynamic::{DynamicError, DynamicQuad},
};
use std::collections::BTreeMap;
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace};
use worldrunner_world_simulation::presentation::extraction::WorldRenderPlan;

/// One frame of a native 2D world: its view, the textures its quads sample, the quads in
/// plan order and the colour the frame clears to.
#[derive(Clone, Debug)]
pub struct WorldSceneContribution {
    pub view: SceneView,
    pub textures: Vec<SceneTexture>,
    pub quads: Vec<DynamicQuad>,
    pub clear: [f64; 4],
}
impl WorldSceneContribution {
    /// Draws the native plan's quads. A quad names its texture by identity, which resolves in the
    /// plan's own textures (UI glyph coverage) and then in `supplied` (the game's sprite
    /// textures); the plan's texture wins when both hold the identity.
    pub fn new(
        plan: WorldRenderPlan,
        supplied: &BTreeMap<u64, SceneTexture>,
    ) -> Result<Self, DynamicError> {
        let camera = plan.camera.ok_or(DynamicError::DeviceLimit(
            "native scene requires an active camera",
        ))?;
        let eye = (camera.eye_meters().as_dvec3() * 1000.)
            .round()
            .as_i64vec3()
            .to_array();
        let mut all = supplied.clone();
        for (id, texture) in plan.textures {
            all.insert(
                id,
                SceneTexture::Texels {
                    format: TexelFormat::Rgba8Unorm,
                    color_space: TextureColorSpace::Linear,
                    width: texture.width,
                    height: texture.height,
                    mips: 1,
                    bytes: texture.bytes.as_ref().into(),
                },
            );
        }
        let mut textures = vec![];
        let mut indices = BTreeMap::new();
        for (id, texture) in all {
            indices.insert(id, textures.len());
            textures.push(texture);
        }
        let mut quads = vec![];
        for (index, q) in plan.quads.into_iter().enumerate() {
            let texture = q
                .texture
                .map(|id| {
                    indices.get(&id).copied().ok_or(DynamicError::InvalidQuad {
                        index,
                        field: "unresolved texture identity",
                    })
                })
                .transpose()?;
            quads.push(DynamicQuad {
                corners: Some(q.corners),
                overlay: q.overlay,
                texture,
                tint: q.color,
                uv_rect: q.uv_rect,
                sorting_layer: q.sorting_layer,
                sorting_order: q.sorting_order,
                ..DynamicQuad::default()
            });
        }
        Ok(Self {
            view: SceneView::from_resolved(camera, eye),
            textures,
            quads,
            clear: plan.clear,
        })
    }
}
