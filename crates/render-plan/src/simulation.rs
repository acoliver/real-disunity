//! Native observations mapped to the shared scene texture/material contract.
use crate::SceneTexture;
use glam::Mat4;
use std::collections::BTreeMap;
use worldrunner_camera::{CameraState, ResolvedView};
use worldrunner_world_simulation::{World, WorldError, presentation::extraction as native};
/// Content identity to scene texture table. Bytes use the ordinary scene texture contract.
#[derive(Clone, Debug, Default)]
pub struct SpriteTextures {
    pub textures: BTreeMap<u64, SceneTexture>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteQuad {
    pub material: crate::BuiltinMaterial,
    pub overlay: bool,
    pub corners: [[f32; 3]; 4],
    pub uv_rect: [f32; 4],
    pub color: [f32; 4],
    pub texture: Option<u64>,
    pub sorting_layer: i32,
    pub sorting_order: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct WorldRenderPlan {
    pub camera: Option<ResolvedView>,
    pub camera_state: Option<CameraState>,
    pub clear: [f64; 4],
    pub quads: Vec<SpriteQuad>,
    pub textures: BTreeMap<u64, SceneTexture>,
}

pub fn extract_world(
    world: &World,
    textures: &SpriteTextures,
    viewport: [u32; 2],
) -> Result<WorldRenderPlan, WorldError> {
    let frame = native::extract_world(world, &native::SpriteTextures::default(), viewport)?;
    Ok(from_presentation(frame, textures))
}
pub fn from_presentation(
    frame: native::WorldRenderPlan,
    textures: &SpriteTextures,
) -> WorldRenderPlan {
    let mut all = textures.textures.clone();
    for (id, t) in frame.textures {
        all.insert(
            id,
            SceneTexture::Texels {
                format: crate::TexelFormat::Rgba8Unorm,
                color_space: crate::TextureColorSpace::Linear,
                width: t.width,
                height: t.height,
                mips: 1,
                bytes: t.bytes,
            },
        );
    }
    WorldRenderPlan {
        camera: frame.camera,
        camera_state: frame.camera_state,
        clear: frame.clear,
        quads: frame.quads.into_iter().map(from_quad).collect(),
        textures: all,
    }
}
fn from_quad(q: native::SpriteQuad) -> SpriteQuad {
    SpriteQuad {
        material: match q.lighting {
            worldrunner_world_simulation::presentation::SpriteLighting::Unlit => {
                crate::BuiltinMaterial::SpriteUnlit
            }
            worldrunner_world_simulation::presentation::SpriteLighting::Global => {
                crate::BuiltinMaterial::SpriteLit
            }
        },
        overlay: q.overlay,
        corners: q.corners,
        uv_rect: q.uv_rect,
        color: q.color,
        texture: q.texture,
        sorting_layer: q.sorting_layer,
        sorting_order: q.sorting_order,
    }
}
#[cfg(feature = "ui")]
fn into_quad(q: SpriteQuad) -> native::SpriteQuad {
    native::SpriteQuad {
        lighting: match q.material {
            crate::BuiltinMaterial::SpriteUnlit => {
                worldrunner_world_simulation::presentation::SpriteLighting::Unlit
            }
            crate::BuiltinMaterial::SpriteLit => {
                worldrunner_world_simulation::presentation::SpriteLighting::Global
            }
        },
        overlay: q.overlay,
        corners: q.corners,
        uv_rect: q.uv_rect,
        color: q.color,
        texture: q.texture,
        sorting_layer: q.sorting_layer,
        sorting_order: q.sorting_order,
    }
}
pub fn extract_sprite(
    renderer: &worldrunner_world_simulation::presentation::SpriteRenderer,
    matrix: Mat4,
) -> Option<SpriteQuad> {
    native::extract_sprite(renderer, matrix).map(from_quad)
}
#[cfg(feature = "ui")]
pub fn append_ui(
    plan: &mut WorldRenderPlan,
    ui: crate::ui::UiRenderPlan,
) -> Result<(), worldrunner_camera::ViewError> {
    let next = plan
        .textures
        .keys()
        .copied()
        .chain(plan.quads.iter().filter_map(|q| q.texture))
        .max()
        .map_or(Some(0), |n| n.checked_add(1))
        .ok_or(worldrunner_camera::ViewError::InvalidWorldPoint)?;
    let mut frame = native::WorldRenderPlan {
        camera: plan.camera,
        camera_state: plan.camera_state,
        clear: plan.clear,
        quads: plan.quads.iter().copied().map(into_quad).collect(),
        textures: BTreeMap::new(),
    };
    native::append_ui_from(&mut frame, crate::ui::into_native(ui)?, next)?;
    *plan = from_presentation(
        frame,
        &SpriteTextures {
            textures: plan.textures.clone(),
        },
    );
    Ok(())
}
