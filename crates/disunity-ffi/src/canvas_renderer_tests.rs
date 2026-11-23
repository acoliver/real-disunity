use super::*;
use crate::abi::disunity_gameobject_create;

fn renderer_world() -> (u64, u64) {
    let world = crate::store::create_world(&mut crate::lock_store()).unwrap();
    let (mut object, mut handle) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        let created = disunity_canvas_renderer_create(world, object, &mut handle);
        assert_eq!(created.code, 0);
    }
    (world, handle)
}

fn held(world: u64, handle: u64) -> CanvasRenderer {
    let store = crate::lock_store();
    let session = store.session(world).unwrap();
    renderer(session, handle).unwrap().1
}

fn set_count(world: u64, handle: u64, count: u32) {
    let value = DisunityCanvasRenderer {
        material_count: count,
        ..DisunityCanvasRenderer::default()
    };
    assert_eq!(
        unsafe { disunity_canvas_renderer_write(world, handle, &value) }.code,
        0
    );
}

#[test]
fn a_material_write_stores_the_world_material_and_clears_it_past_the_count() {
    let (world, handle) = renderer_world();
    let id = (2 << 32) | 4;
    set_count(world, handle, 2);
    assert_eq!(
        disunity_canvas_renderer_material_write(world, handle, 0, 0, 7, id).code,
        0
    );
    let stored = held(world, handle);
    assert_eq!(stored.materials[0], 7);
    assert_eq!(stored.world_material, Some(MaterialId::from_bits(id)));

    for (pop, index) in [(0, 1), (1, 0)] {
        let write = disunity_canvas_renderer_material_write(world, handle, pop, index, 8, id);
        assert_ne!(
            write.code, 0,
            "pop {pop} slot {index} holds no world material"
        );
    }
    assert_eq!(
        held(world, handle),
        stored,
        "a rejected write changes nothing"
    );

    set_count(world, handle, 0);
    let emptied = held(world, handle);
    assert_eq!((emptied.materials[0], emptied.world_material), (0, None));

    set_count(world, handle, 1);
    assert_eq!(
        disunity_canvas_renderer_material_write(world, handle, 0, 0, 7, id).code,
        0
    );
    assert_eq!(
        disunity_canvas_renderer_material_write(world, handle, 0, 0, 0, 0).code,
        0
    );
    assert_eq!(
        held(world, handle).world_material,
        None,
        "zero clears the slot"
    );
    crate::store::destroy_world(&mut crate::lock_store(), world).unwrap();
}
