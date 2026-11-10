//! A render texture's shape: its dimension and volume depth, what the world accepts and what
//! the snapshot carries to the host.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode, encode};
use worldrunner_world_simulation::graphics::{
    GraphicsError, GraphicsSnapshot, MAX_VOLUME_DEPTH, OffscreenWork, WorldRenderTexture,
    WorldSampler, dimension,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn shaped(size: [u32; 2], dimension: u32, volume_depth: u32) -> WorldRenderTexture {
    WorldRenderTexture {
        width: size[0],
        height: size[1],
        format: 48,
        depth_bits: 0,
        mip_count: 1,
        srgb: false,
        dimension,
        volume_depth,
        sampler: WorldSampler::default(),
        generation: 0,
    }
}

fn refused(world: &mut World, texture: WorldRenderTexture) -> bool {
    matches!(
        world.create_render_texture(texture),
        Err(WorldError::Graphics(GraphicsError::InvalidRenderTexture))
    )
}

#[test]
fn each_dimension_keeps_its_shape() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for texture in [
        shaped([8, 4], dimension::TEX_2D, 1),
        shaped([8, 4], dimension::TEX_3D, 6),
        shaped([8, 8], dimension::CUBE, 1),
        shaped([8, 4], dimension::TEX_2D_ARRAY, 1),
        shaped([8, 4], dimension::TEX_2D_ARRAY, MAX_VOLUME_DEPTH),
        shaped([8, 8], dimension::CUBE_ARRAY, 12),
    ] {
        let id = world.create_render_texture(texture).unwrap();
        let held = world.render_texture(id).unwrap();
        assert_eq!(
            [held.dimension, held.volume_depth],
            [texture.dimension, texture.volume_depth]
        );
    }
}

#[test]
fn a_shape_that_does_not_fit_its_dimension_is_refused() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    for (texture, why) in [
        (
            shaped([8, 4], dimension::TEX_2D, 2),
            "a 2D texture has one slice",
        ),
        (shaped([8, 4], dimension::CUBE, 1), "a cube is square"),
        (shaped([8, 8], dimension::CUBE, 6), "a cube has one slice"),
        (shaped([8, 4], dimension::TEX_3D, 0), "a volume has a slice"),
        (
            shaped([8, 4], dimension::TEX_2D_ARRAY, MAX_VOLUME_DEPTH + 1),
            "too many layers",
        ),
        (
            shaped([8, 8], dimension::CUBE_ARRAY, 8),
            "a cube array has whole cubes",
        ),
        (
            shaped([8, 4], dimension::CUBE_ARRAY, 6),
            "a cube array is square",
        ),
        (shaped([8, 4], 1, 1), "no texture is of any dimension"),
        (shaped([8, 4], 7, 1), "no dimension past the cube array"),
    ] {
        assert!(refused(&mut world, texture), "{why}");
    }
}

#[test]
fn an_update_can_reshape_and_is_checked_the_same_way() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let id = world
        .create_render_texture(shaped([8, 8], dimension::TEX_2D, 1))
        .unwrap();
    world
        .update_render_texture(id, shaped([8, 8], dimension::TEX_3D, 4))
        .unwrap();
    assert_eq!(world.render_texture(id).unwrap().volume_depth, 4);
    assert!(
        world
            .update_render_texture(id, shaped([8, 8], dimension::TEX_2D, 4))
            .is_err()
    );
}

#[test]
fn the_snapshot_carries_the_shape_to_the_host() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let id = world
        .create_render_texture(shaped([8, 4], dimension::TEX_2D_ARRAY, 3))
        .unwrap();
    world
        .record_offscreen(OffscreenWork::Blit {
            source: worldrunner_world_simulation::graphics::TextureRef::Render(id),
            target: id,
            material: None,
            pass: -1,
        })
        .unwrap();
    let taken = GraphicsSnapshot::take(&mut world).unwrap();
    let decoded = decode(&encode(&taken).unwrap()).unwrap();
    let held = &decoded.render_textures[&id];
    assert_eq!(
        [held.dimension, held.volume_depth],
        [dimension::TEX_2D_ARRAY, 3]
    );
}
