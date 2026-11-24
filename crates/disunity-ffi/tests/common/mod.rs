//! Helpers the graphics ABI tests share: a world, a mesh renderer, its slot blocks and the shader
//! property calls that write and read a float.
use worldrunner_disunity_ffi::{abi::*, graphics_properties::*, mesh_renderer::*};

pub const SCOPE_BLOCK: u32 = 2;
pub const SCOPE_SLOT_BLOCK: u32 = 4;

pub fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

pub fn text(name: &str) -> (*const u8, u32) {
    (name.as_ptr(), u32::try_from(name.len()).unwrap())
}

pub fn renderer(world: u64) -> u64 {
    let (mut object, mut renderer) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_mesh_renderer_create(world, object, 0, &mut renderer).code,
            0
        );
    }
    renderer
}

pub fn read_property(world: u64, scope: u32, target: u64, name: &str) -> DisunityPropertyRead {
    let (bytes, length) = text(name);
    let mut value = DisunityPropertyRead::default();
    let result =
        unsafe { disunity_shader_property_read(world, scope, target, bytes, length, &mut value) };
    assert_eq!(result.code, 0);
    value
}

pub fn set_float(world: u64, scope: u32, target: u64, name: &str, value: f32) -> DisunityResult {
    let (bytes, length) = text(name);
    unsafe { disunity_shader_float_write(world, scope, target, bytes, length, value) }
}

pub fn slot_block(world: u64, renderer: u64, material_index: u32) -> u64 {
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
