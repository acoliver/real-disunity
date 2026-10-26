//! Native world contributions use the resident scene's existing unlit quad pipeline.
use crate::{
    SceneView,
    dynamic::{DynamicError, DynamicQuad},
};
use worldrunner_render_plan::{SceneTexture, simulation::WorldRenderPlan};

pub struct WorldSceneContribution {
    pub view: SceneView,
    pub textures: Vec<SceneTexture>,
    pub quads: Vec<DynamicQuad>,
    pub clear: [f64; 4],
}
impl WorldSceneContribution {
    pub fn new(plan: WorldRenderPlan) -> Result<Self, DynamicError> {
        let camera = plan.camera.ok_or(DynamicError::DeviceLimit(
            "native scene requires an active camera",
        ))?;
        let eye = (camera.eye_meters().as_dvec3() * 1000.)
            .round()
            .as_i64vec3()
            .to_array();
        let mut textures = vec![];
        let mut indices = std::collections::BTreeMap::new();
        for (id, texture) in plan.textures {
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
