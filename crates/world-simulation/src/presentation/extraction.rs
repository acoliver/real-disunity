//! Pathless extraction from the sole native world into shared scene geometry.
use crate::presentation::CoverageTexture as SceneTexture;
use crate::presentation::{Sprite, sprite_corners};
use crate::{ComponentDesc, World, WorldError};
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ResolvedView, ViewConvention, ViewHandedness, Viewport,
    resolve,
};

/// Content identity to scene texture table. Bytes use the ordinary scene texture contract.
#[derive(Clone, Debug, Default)]
pub struct SpriteTextures {
    pub textures: BTreeMap<u64, SceneTexture>,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpriteQuad {
    pub lighting: super::SpriteLighting,
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
    let snapshot = world.snapshot();
    let lighting = super::lighting::GlobalLighting::from_world(world)?;
    let mut result = WorldRenderPlan {
        camera: None,
        camera_state: None,
        clear: [0., 0., 0., 1.],
        quads: vec![],
        textures: textures.textures.clone(),
    };
    let mut camera_depth = None;
    for component in snapshot.components {
        if !world.active_in_hierarchy(component.object)? {
            continue;
        }
        let basis = Mat4::from_cols_array(&[
            1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1.,
        ]);
        let matrix = basis * world.world_matrix(component.object)?;
        match component.desc {
            ComponentDesc::Sprite(mut renderer) if renderer.enabled => {
                renderer.color = lighting.color(&renderer);
                if let Some(mut quad) =
                    extract_sprite(&renderer, world.world_matrix(component.object)?)
                {
                    quad.lighting = renderer.lighting;
                    result.quads.push(quad);
                }
            }

            ComponentDesc::Tilemap(map) if map.enabled => {
                append_tiles(&mut result.quads, map, matrix);
            }
            ComponentDesc::Camera(camera)
                if camera.enabled && camera_depth.is_none_or(|d| camera.depth > d) =>
            {
                let state = camera_state(world, component.object, camera, matrix, basis, viewport)?;
                result.camera = Some(resolve(&state).map_err(|_| {
                    WorldError::InvalidComponent(crate::ComponentError::InvalidPresentation)
                })?);
                result.camera_state = Some(state);
                result.clear = camera.clear_color()?;
                camera_depth = Some(camera.depth);
            }
            _ => {}
        }
    }
    result
        .quads
        .sort_by_key(|q| (q.overlay, q.sorting_layer, q.sorting_order));
    Ok(result)
}
fn append_tiles(quads: &mut Vec<SpriteQuad>, map: crate::presentation::Tilemap, matrix: Mat4) {
    for tile in map.tiles {
        let color = std::array::from_fn(|i| map.color[i] * tile.color[i]);
        quads.push(quad(
            tile.sprite,
            matrix * tile.transform,
            color,
            false,
            false,
            [map.sorting_layer, map.sorting_order],
        ));
    }
}
fn camera_state(
    world: &World,
    object: crate::ObjectHandle,
    camera: crate::presentation::Camera,
    matrix: Mat4,
    basis: Mat4,
    viewport: [u32; 2],
) -> Result<CameraState, WorldError> {
    let rotation = world.world_rotation(object)?;
    let state = CameraState {
        pose: CameraPose {
            eye_meters: matrix.w_axis.truncate().as_dvec3(),
            forward: basis.transform_vector3(rotation * Vec3::Z).as_dvec3(),
            up: basis.transform_vector3(rotation * Vec3::Y).as_dvec3(),
        },
        lens: CameraLens::Orthographic {
            half_height_meters: f64::from(camera.half_height),
            near_meters: f64::from(camera.near),
            far_meters: f64::from(camera.far),
        },
        viewport: Viewport {
            width: viewport[0],
            height: viewport[1],
        },
        convention: ViewConvention {
            handedness: ViewHandedness::DirectXLeftHanded,
        },
    };
    Ok(state)
}
fn quad(
    sprite: Sprite,
    matrix: Mat4,
    color: [f32; 4],
    flip_x: bool,
    flip_y: bool,
    sorting: [i32; 2],
) -> SpriteQuad {
    let [x, y, width, height] = sprite.rect;
    let [tw, th] = glam::UVec2::from_array(sprite.texture_size)
        .as_vec2()
        .to_array();
    let mut uv_rect = [
        x / tw,
        1. - (y + height) / th,
        (x + width) / tw,
        1. - y / th,
    ];
    if flip_x {
        uv_rect.swap(0, 2);
    }
    if flip_y {
        uv_rect.swap(1, 3);
    }
    let mut corners = sprite_corners(sprite, matrix, false, false);
    if flip_x || flip_y {
        let [left, bottom, right, top] = sprite.bounds();
        let [left, right] = if flip_x {
            [-right, -left]
        } else {
            [left, right]
        };
        let [bottom, top] = if flip_y {
            [-top, -bottom]
        } else {
            [bottom, top]
        };
        corners = [[left, bottom], [right, bottom], [right, top], [left, top]]
            .map(|[x, z]| matrix.transform_point3(Vec3::new(x, z, 0.)).to_array());
    }
    SpriteQuad {
        lighting: super::SpriteLighting::Unlit,
        overlay: false,
        corners,
        uv_rect,
        color,
        texture: Some(sprite.texture),
        sorting_layer: sorting[0],
        sorting_order: sorting[1],
    }
}

#[cfg(feature = "ui")]
pub fn append_ui(
    plan: &mut WorldRenderPlan,
    ui: crate::ui::contribution::UiRenderPlan,
) -> Result<(), worldrunner_camera::ViewError> {
    let next = plan
        .textures
        .keys()
        .next_back()
        .copied()
        .map_or(Some(0), |n| n.checked_add(1))
        .ok_or(worldrunner_camera::ViewError::InvalidWorldPoint)?;
    append_ui_from(plan, ui, next)
}

#[cfg(feature = "ui")]
pub fn append_ui_from(
    plan: &mut WorldRenderPlan,
    ui: crate::ui::contribution::UiRenderPlan,
    mut next: u64,
) -> Result<(), worldrunner_camera::ViewError> {
    if let Some(camera) = plan.camera {
        if ui.layout.viewport != glam::UVec2::new(camera.viewport().width, camera.viewport().height)
        {
            return Err(worldrunner_camera::ViewError::InvalidScreenPoint);
        }
    } else if ui
        .texts
        .iter()
        .any(|text| text.overlay && !text.quads.is_empty())
    {
        return Err(worldrunner_camera::ViewError::InvalidWorldPoint);
    }
    for text in ui.texts {
        let Some(texture) = text.texture else {
            continue;
        };
        let id = next;
        next = next
            .checked_add(1)
            .ok_or(worldrunner_camera::ViewError::InvalidWorldPoint)?;
        let texture = glyph_texture(&texture);
        plan.textures.insert(id, texture);
        for glyph in text.quads {
            let mut corners = glyph.corners.map(|[x, y, z]| [x, -z, y]);
            if text.overlay {
                let camera = plan
                    .camera
                    .ok_or(worldrunner_camera::ViewError::InvalidWorldPoint)?;
                let height = glam::UVec2::new(camera.viewport().width, camera.viewport().height)
                    .as_vec2()
                    .y;
                corners = glyph.corners;
                for corner in &mut corners {
                    *corner = camera
                        .screen_to_world(Vec3::new(corner[0], height - corner[1], 0.0))?
                        .to_array();
                }
            }
            let uv = glyph.uv;
            plan.quads.push(SpriteQuad {
                lighting: super::SpriteLighting::Unlit,
                overlay: text.overlay,
                corners,
                uv_rect: [uv[0][0], uv[2][1], uv[2][0], uv[0][1]],
                color: text.color,
                texture: Some(id),
                sorting_layer: text.sorting_layer,
                sorting_order: text.sorting_order,
            });
        }
    }
    plan.quads
        .sort_by_key(|q| (q.overlay, q.sorting_layer, q.sorting_order));
    Ok(())
}

#[cfg(feature = "ui")]
fn glyph_texture(texture: &SceneTexture) -> SceneTexture {
    SceneTexture {
        width: texture.width,
        height: texture.height,
        bytes: texture
            .bytes
            .iter()
            .flat_map(|&alpha| [255, 255, 255, alpha])
            .collect::<Vec<_>>()
            .into_boxed_slice(),
    }
}

/// Extracts an immutable native renderer observation, including managed-host copy-outs.
pub fn extract_sprite(
    renderer: &crate::presentation::SpriteRenderer,
    matrix: Mat4,
) -> Option<SpriteQuad> {
    if !renderer.enabled {
        return None;
    }
    let sprite = renderer.sprite?;
    let basis = Mat4::from_cols_array(&[
        1., 0., 0., 0., 0., 0., 1., 0., 0., -1., 0., 0., 0., 0., 0., 1.,
    ]);
    let mut result = quad(
        sprite,
        basis * matrix,
        renderer.color,
        renderer.flip_x,
        renderer.flip_y,
        [renderer.sorting_layer, renderer.sorting_order],
    );
    result.lighting = renderer.lighting;
    Some(result)
}
