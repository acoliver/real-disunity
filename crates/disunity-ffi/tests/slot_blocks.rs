//! Per-slot property blocks through the ABI: clearing one slot leaves the others, and writing the
//! renderer's settings leaves every block in place.
use worldrunner_disunity_ffi::{abi::*, graphics_properties::*, mesh_renderer::*};

const SCOPE_BLOCK: u32 = 2;
const SCOPE_SLOT_BLOCK: u32 = 4;

fn new_renderer() -> (u64, u64) {
    let (mut world, mut object, mut renderer) = (0, 0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_mesh_renderer_create(world, object, 0, &mut renderer).code,
            0
        );
    }
    (world, renderer)
}

fn slot_block(world: u64, renderer: u64, material_index: u32) -> u64 {
    let mut target = 0;
    unsafe {
        assert_eq!(
            disunity_mesh_renderer_slot_block_target(world, renderer, material_index, &mut target)
                .code,
            0
        );
    }
    target
}

fn set_fade(world: u64, scope: u32, target: u64, value: f32) {
    let name = "_Fade";
    let result = unsafe {
        disunity_shader_float_write(
            world,
            scope,
            target,
            name.as_ptr(),
            u32::try_from(name.len()).unwrap(),
            value,
        )
    };
    assert_eq!(result.code, 0);
}

/// The value of `_Fade` in the block, or `None` when the block does not hold it.
fn fade(world: u64, scope: u32, target: u64) -> Option<f32> {
    let name = "_Fade";
    let mut value = DisunityPropertyRead::default();
    let result = unsafe {
        disunity_shader_property_read(
            world,
            scope,
            target,
            name.as_ptr(),
            u32::try_from(name.len()).unwrap(),
            &mut value,
        )
    };
    assert_eq!(result.code, 0);
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
