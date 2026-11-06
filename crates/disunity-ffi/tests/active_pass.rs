//! `disunity_material_set_pass` validates its arguments and records the pass in the world.
use worldrunner_disunity_ffi::{abi::*, graphics::*, with_native_world};
use worldrunner_world_simulation::graphics::{ActivePass, MaterialId};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn material(world: u64) -> u64 {
    let name = "Icons";
    let (mut shader, mut id) = (0, 0);
    unsafe {
        let created = disunity_shader_create(world, name.as_ptr(), 5, &mut shader);
        assert_eq!(created.code, 0);
        assert_eq!(disunity_material_create(world, shader, &mut id).code, 0);
    }
    id
}

fn read(world: u64) -> DisunityActivePass {
    let mut held = DisunityActivePass::default();
    unsafe { assert_eq!(disunity_active_pass_read(world, &mut held).code, 0) };
    held
}

#[test]
fn a_world_without_a_pass_reads_as_inactive() {
    let world = new_world();
    assert_eq!(read(world), DisunityActivePass::default());
    assert_eq!(read(world).active, 0);
}

#[test]
fn a_pass_reaches_the_world_and_reads_back() {
    let world = new_world();
    let id = material(world);
    assert_eq!(disunity_material_set_pass(world, id, 1).code, 0);
    with_native_world(world, |native| {
        assert_eq!(
            native.active_pass(),
            Some(ActivePass {
                material: MaterialId::from_bits(id),
                pass: 1
            })
        );
    })
    .unwrap();
    assert_eq!(
        read(world),
        DisunityActivePass {
            material: id,
            pass: 1,
            active: 1
        }
    );
    assert_eq!(disunity_material_set_pass(world, id, 0).code, 0);
    assert_eq!(read(world).pass, 0);
}

#[test]
fn an_unknown_material_is_refused_and_keeps_the_earlier_pass() {
    let world = new_world();
    let id = material(world);
    assert_eq!(disunity_material_set_pass(world, id, 2).code, 0);
    assert_ne!(disunity_material_set_pass(world, 0, 0).code, 0);
    assert_ne!(disunity_material_set_pass(world, id + 1, 0).code, 0);
    assert_eq!(read(world).pass, 2);
    assert_eq!(disunity_material_destroy(world, id).code, 0);
    assert_ne!(disunity_material_set_pass(world, id, 0).code, 0);
    assert_eq!(read(world).active, 0);
}

#[test]
fn an_unknown_world_or_a_null_output_is_refused() {
    let world = new_world();
    assert_ne!(disunity_material_set_pass(0, 1, 0).code, 0);
    assert_ne!(disunity_material_set_pass(u64::MAX, 1, 0).code, 0);
    unsafe {
        assert_ne!(
            disunity_active_pass_read(world, std::ptr::null_mut()).code,
            0
        );
        let mut held = DisunityActivePass::default();
        assert_ne!(disunity_active_pass_read(0, &mut held).code, 0);
    }
}
