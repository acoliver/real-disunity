//! Per-slot property blocks through the ABI: clearing one slot leaves the others, and writing the
//! renderer's settings leaves every block in place.
mod common;

use common::{
    SCOPE_BLOCK, SCOPE_SLOT_BLOCK, new_world, read_property, renderer, set_float, slot_block,
};
use worldrunner_disunity_ffi::{graphics_properties::*, mesh_renderer::*};

fn new_renderer() -> (u64, u64) {
    let world = new_world();
    (world, renderer(world))
}

fn set_fade(world: u64, scope: u32, target: u64, value: f32) {
    assert_eq!(set_float(world, scope, target, "_Fade", value).code, 0);
}

/// The value of `_Fade` in the block, or `None` when the block does not hold it.
fn fade(world: u64, scope: u32, target: u64) -> Option<f32> {
    let value = read_property(world, scope, target, "_Fade");
    (value.present != 0).then_some(value.float_value)
}

#[test]
fn clearing_one_slots_block_keeps_the_blocks_of_the_other_slots() {
    let (world, renderer) = new_renderer();
    let slots = [0, 1, 2].map(|index| slot_block(world, renderer, index));
    for (slot, value) in slots.iter().zip([0.25, 0.5, 0.75]) {
        set_fade(world, SCOPE_SLOT_BLOCK, *slot, value);
    }
    let cleared = disunity_shader_properties_clear(world, SCOPE_SLOT_BLOCK, slots[1]);
    assert_eq!(cleared.code, 0);
    let held = slots.map(|slot| fade(world, SCOPE_SLOT_BLOCK, slot));
    assert_eq!(held, [Some(0.25), None, Some(0.75)]);
}

#[test]
fn writing_the_renderer_settings_keeps_its_all_slot_block_and_every_slot_block() {
    let (world, renderer) = new_renderer();
    let slots = [0, 3].map(|index| slot_block(world, renderer, index));
    set_fade(world, SCOPE_BLOCK, renderer, 0.125);
    set_fade(world, SCOPE_SLOT_BLOCK, slots[0], 0.5);
    set_fade(world, SCOPE_SLOT_BLOCK, slots[1], 0.75);

    let mut settings: DisunityMeshRenderer = unsafe { std::mem::zeroed() };
    unsafe {
        assert_eq!(
            disunity_mesh_renderer_read(world, renderer, &mut settings).code,
            0
        );
    }
    settings.enabled = 0;
    settings.material_count = 4;
    unsafe {
        assert_eq!(
            disunity_mesh_renderer_write(world, renderer, &settings).code,
            0
        );
    }

    assert_eq!(fade(world, SCOPE_BLOCK, renderer), Some(0.125));
    assert_eq!(fade(world, SCOPE_SLOT_BLOCK, slots[0]), Some(0.5));
    assert_eq!(fade(world, SCOPE_SLOT_BLOCK, slots[1]), Some(0.75));
}
