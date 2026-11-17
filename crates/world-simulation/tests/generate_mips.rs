//! A render texture's two ways of getting its mips: the `auto_generate_mips` flag that the host
//! follows on every draw, and the `GenerateMips` record a script asks for. Both travel to the host in
//! the graphics snapshot; the record is refused for a texture with no mips to fill.
use worldrunner_world_simulation::graphics::snapshot_wire::{
    decode, decode_work, encode, encode_work,
};
use worldrunner_world_simulation::graphics::{
    GraphicsError, GraphicsSnapshot, OffscreenWork, RenderTextureId, WorldRenderTexture,
    WorldSampler,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn texture(mip_count: u32, auto_generate_mips: bool) -> WorldRenderTexture {
    WorldRenderTexture {
        width: 16,
        height: 8,
        format: 8,
        depth_bits: 0,
        mip_count,
        srgb: false,
        dimension: 2,
        volume_depth: 1,
        auto_generate_mips,
        sampler: WorldSampler::default(),
        generation: 0,
    }
}

fn generate(texture: RenderTextureId) -> OffscreenWork {
    OffscreenWork::GenerateMips { texture }
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

#[test]
fn the_flag_is_held_and_follows_an_update() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let id = world.create_render_texture(texture(5, true)).unwrap();
    assert!(world.render_texture(id).unwrap().auto_generate_mips);
    world.update_render_texture(id, texture(5, false)).unwrap();
    assert!(!world.render_texture(id).unwrap().auto_generate_mips);
}

#[test]
fn the_flag_and_the_record_survive_the_snapshot_wire_in_call_order() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let flagged = world.create_render_texture(texture(5, true)).unwrap();
    let manual = world.create_render_texture(texture(3, false)).unwrap();
    world.record_offscreen(generate(manual)).unwrap();
    world.record_offscreen(generate(flagged)).unwrap();
    let taken = GraphicsSnapshot::take(&mut world).unwrap();
    let decoded = decode(&encode(&taken).unwrap()).unwrap();
    assert!(decoded.render_textures[&flagged].auto_generate_mips);
    assert!(!decoded.render_textures[&manual].auto_generate_mips);
    assert_eq!(decoded.work, vec![generate(manual), generate(flagged)]);
}

#[test]
fn a_single_record_encodes_under_tag_eighteen_and_round_trips() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let id = world.create_render_texture(texture(2, false)).unwrap();
    let bytes = encode_work(&generate(id)).unwrap();
    assert_eq!(bytes[0], 18);
    assert_eq!(decode_work(&bytes).unwrap(), generate(id));
    assert!(decode_work(&bytes[..bytes.len() - 1]).is_err());
}

#[test]
fn a_texture_with_no_mips_to_fill_is_refused() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let flat = world.create_render_texture(texture(1, false)).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(generate(flat))),
        GraphicsError::InvalidOffscreen
    );
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn a_destroyed_texture_is_refused_by_name() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let id = world.create_render_texture(texture(4, false)).unwrap();
    world.destroy_render_texture(id).unwrap();
    assert_eq!(
        graphics_error(world.record_offscreen(generate(id))),
        GraphicsError::StaleRenderTexture
    );
    assert!(world.offscreen_work().is_empty());
}

#[test]
fn the_record_names_its_texture_so_a_destroyed_one_stays_in_the_snapshot() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    let id = world.create_render_texture(texture(4, false)).unwrap();
    let other = world.create_render_texture(texture(4, false)).unwrap();
    world.record_offscreen(generate(id)).unwrap();
    assert!(generate(id).names_render_texture(id));
    assert!(!generate(id).names_render_texture(other));
    world.destroy_render_texture(id).unwrap();
    let taken = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(taken.render_textures[&id].mip_count, 4);
}
