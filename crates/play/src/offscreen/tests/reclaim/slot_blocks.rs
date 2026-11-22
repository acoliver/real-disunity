//! What only a slot block names counts as named: its stored textures are uploaded and its render
//! textures are not given back.
use super::*;
use glam::{Quat, Vec3};
use std::collections::BTreeMap;
use worldrunner_world_simulation::MeshRenderer;
use worldrunner_world_simulation::graphics::ShaderProperties;
use worldrunner_world_simulation::presentation::Camera;

use super::super::support::{camera, renderer_with};

fn holding(texture: TextureRef) -> ShaderProperties {
    let mut block = ShaderProperties::default();
    let value = WorldValue::Texture("_Held".to_owned(), texture_slot(texture));
    block.set(value).unwrap();
    block
}

/// Adds a renderer of the quad whose slot 2, which has no material, holds `slot` and whose
/// all-slot block holds `all`, then a cull of every layer, because a snapshot lists the renderers
/// only for a frame whose cull found them.
fn add_renderer(world: &mut World, all: Option<ShaderProperties>, slot: ShaderProperties) {
    let renderer = MeshRenderer {
        property_block: all,
        slot_blocks: BTreeMap::from([(2, slot)]),
        ..MeshRenderer::default()
    };
    renderer_with(world, (0, true), MESH, renderer);
    let culling = camera(world, Camera::default(), Vec3::ZERO, Quat::IDENTITY);
    let cull = OffscreenWork::Cull {
        id: 900,
        camera: culling,
        culling_mask: u32::MAX,
        planes: Vec::new(),
        options: 0,
    };
    world.record_offscreen(cull).unwrap();
}

#[test]
fn a_stored_texture_only_a_slot_block_names_is_uploaded() {
    let mut room = Room::new(64 << 20);
    let mut world = world();
    add_renderer(
        &mut world,
        Some(holding(TextureRef::Asset(B))),
        holding(TextureRef::Asset(A)),
    );
    let report = room.adapt(&snapshot(&mut world));
    assert_eq!(over_budget(&report), 0, "{:?}", report.causes);
    let mut uploaded = room.textures.uploads.borrow().clone();
    uploaded.sort_unstable();
    assert_eq!(uploaded, [A, B]);
}

/// Plays three frames in a room of `budget` bytes and returns the room with the result and the
/// reclaim of the third. `b` was named longest ago and no record of the third frame draws into
/// it, but a slot block of a renderer in that frame holds it.
fn three_frames(budget: u64) -> (Room, Result<(), OffscreenError>, Reclaimed, [TextureId; 3]) {
    let mut room = Room::new(budget);
    let mut targets = Targets::new();
    let (a, b, c) = (targets.a, targets.b, targets.c);
    let frames = [targets.naming(&[a, b]), targets.naming(&[a])];
    room.frame(&frames[0]).0.unwrap();
    room.frame(&frames[1]).0.unwrap();
    add_renderer(&mut targets.world, None, holding(TextureRef::Render(b)));
    let third = targets.naming(&[c]);
    let (built, reclaimed) = room.frame(&third);
    (room, built, reclaimed, [a, b, c].map(Targets::id))
}

#[test]
fn a_render_texture_only_a_slot_block_holds_is_named_and_stays() {
    // With room to spare nothing is given back; the budget that follows is 256 bytes short of
    // that, which one 8 x 8 render texture frees.
    let (roomy, built, reclaimed, _) = three_frames(64 << 20);
    built.unwrap();
    assert_eq!(reclaimed.targets, 0);
    let (room, built, reclaimed, [_, b, c]) = three_frames(roomy.runtime.live_bytes() - 256);
    built.unwrap();
    assert_eq!(
        reclaimed.targets, 1,
        "a budget 256 bytes short is one 8 x 8 render texture short"
    );
    assert_eq!(room.held(), [b, c]);
}
