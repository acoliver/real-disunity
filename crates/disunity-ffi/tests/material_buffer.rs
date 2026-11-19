//! `disunity_material_set_buffer` binds a live compute buffer to a property name of a material in
//! the world and refuses a null or empty name, an unknown world, material or buffer, and a
//! destroyed buffer without changing the material. A buffer id of 0 removes the binding.
use worldrunner_disunity_ffi::{abi::*, compute::*, graphics::*, with_native_world};
use worldrunner_world_simulation::graphics::{ComputeBufferId, MaterialId};

fn new_world() -> u64 {
    let mut world = 0;
    unsafe { assert_eq!(disunity_world_create(&mut world).code, 0) };
    world
}

fn material(world: u64) -> u64 {
    let (mut shader, mut id) = (0, 0);
    unsafe {
        let created = disunity_shader_create(world, b"Instanced".as_ptr(), 9, &mut shader);
        assert_eq!(created.code, 0);
        assert_eq!(disunity_material_create(world, shader, &mut id).code, 0);
    }
    id
}

fn buffer(world: u64) -> u64 {
    let desc = DisunityComputeBufferDesc {
        count: 4,
        stride: 4,
        kind: 0,
    };
    let mut id = 0;
    unsafe {
        assert_eq!(
            disunity_compute_buffer_create(world, &desc, &mut id).code,
            0
        )
    };
    id
}

fn bind(world: u64, material: u64, name: &str, buffer: u64) -> i32 {
    let length = u32::try_from(name.len()).unwrap();
    unsafe { disunity_material_set_buffer(world, material, name.as_ptr(), length, buffer) }.code
}

fn bound(world: u64, material: u64) -> Vec<(String, u64)> {
    with_native_world(world, |native| {
        native
            .material(MaterialId::from_bits(material))
            .unwrap()
            .buffers
            .iter()
            .map(|(name, id)| (name.clone(), id.to_bits()))
            .collect()
    })
    .unwrap()
}

#[test]
fn a_live_buffer_is_bound_to_the_name_and_the_world_holds_it() {
    let world = new_world();
    let (material, first, second) = (material(world), buffer(world), buffer(world));
    assert_eq!(bind(world, material, "_Properties", first), 0);
    assert_eq!(
        bound(world, material),
        vec![("_Properties".to_owned(), first)]
    );
    assert_eq!(bind(world, material, "_Properties", second), 0);
    assert_eq!(
        bound(world, material),
        vec![("_Properties".to_owned(), second)]
    );
    with_native_world(world, |native| {
        assert!(
            native
                .compute_buffer(ComputeBufferId::from_bits(second))
                .is_ok()
        );
    })
    .unwrap();
}

#[test]
fn a_buffer_id_of_zero_removes_the_binding() {
    let world = new_world();
    let (material, live) = (material(world), buffer(world));
    assert_eq!(bind(world, material, "_Properties", live), 0);
    assert_eq!(bind(world, material, "_Properties", 0), 0);
    assert!(bound(world, material).is_empty());
}

#[test]
fn an_unknown_buffer_material_or_world_is_refused_and_changes_nothing() {
    let world = new_world();
    let (material, live, gone) = (material(world), buffer(world), buffer(world));
    assert_eq!(bind(world, material, "_Properties", live), 0);
    assert_eq!(disunity_compute_buffer_destroy(world, gone).code, 0);
    assert_ne!(bind(world, material, "_Properties", gone), 0, "destroyed");
    assert_ne!(bind(world, material, "_Other", live + 1_000), 0, "unknown");
    assert_ne!(bind(world, material + 1_000, "_Properties", live), 0);
    assert_ne!(bind(world, 0, "_Properties", live), 0);
    assert_ne!(bind(world + 999, material, "_Properties", live), 0);
    assert_eq!(
        bound(world, material),
        vec![("_Properties".to_owned(), live)]
    );
    assert_eq!(disunity_material_destroy(world, material).code, 0);
    assert_ne!(
        bind(world, material, "_Properties", live),
        0,
        "destroyed material"
    );
}

#[test]
fn a_null_or_empty_name_is_refused() {
    let world = new_world();
    let (material, live) = (material(world), buffer(world));
    unsafe {
        let null = disunity_material_set_buffer(world, material, std::ptr::null(), 4, live);
        assert_ne!(null.code, 0);
        let empty = disunity_material_set_buffer(world, material, b"x".as_ptr(), 0, live);
        assert_ne!(empty.code, 0);
    }
    assert!(bound(world, material).is_empty());
}
