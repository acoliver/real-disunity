//! Structured buffers bound to a material by property name: stored by the world, refused when
//! they name nothing alive, copied once into the snapshot of a frame that draws the material, and
//! carried by the snapshot's wire encoding.
use worldrunner_world_simulation::graphics::snapshot_wire::{decode, encode};
use worldrunner_world_simulation::graphics::{
    ComputeBufferId, DrawTopology, GraphicsError, GraphicsSnapshot, MaterialEdit, MaterialId,
    OffscreenWork, WorldShader,
};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn material(world: &mut World) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: "Custom/Instanced".into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

fn buffer(world: &mut World, fill: u8) -> ComputeBufferId {
    let id = world.create_compute_buffer(4, 4, 0).unwrap();
    world.set_compute_buffer_data(id, 0, &[fill; 16]).unwrap();
    id
}

fn graphics_error(result: Result<(), WorldError>) -> GraphicsError {
    match result {
        Err(WorldError::Graphics(error)) => error,
        other => panic!("expected a graphics error, got {other:?}"),
    }
}

fn draw_mesh(material: MaterialId, instances: u32) -> OffscreenWork {
    OffscreenWork::DrawMesh {
        mesh: 1 << 41,
        matrix: [0.0; 16],
        material,
        submesh: 0,
        pass: 0,
        block: None,
        instances,
    }
}

fn draw_procedural(material: MaterialId) -> OffscreenWork {
    OffscreenWork::DrawProcedural {
        matrix: [0.0; 16],
        material,
        pass: 0,
        topology: DrawTopology::TriangleList,
        vertex_count: 3,
        instance_count: 2,
    }
}

#[test]
fn a_material_keeps_the_buffers_bound_to_it_sorted_by_name() {
    let mut world = world();
    let material = material(&mut world);
    let (first, second) = (buffer(&mut world, 1), buffer(&mut world, 2));
    let generation = world.material(material).unwrap().generation;
    world
        .set_material_buffer(material, "_Properties".into(), Some(first))
        .unwrap();
    world
        .set_material_buffer(material, "_Colors".into(), Some(second))
        .unwrap();
    let held = world.material(material).unwrap();
    assert_eq!(
        held.buffers,
        vec![
            ("_Colors".to_owned(), second),
            ("_Properties".to_owned(), first)
        ]
    );
    assert!(held.generation > generation, "a binding is an edit");
    world
        .set_material_buffer(material, "_Properties".into(), Some(second))
        .unwrap();
    assert_eq!(
        world.material(material).unwrap().buffers,
        vec![
            ("_Colors".to_owned(), second),
            ("_Properties".to_owned(), second)
        ],
        "a name takes the buffer it was last given"
    );
}

#[test]
fn a_binding_to_a_buffer_that_is_not_alive_is_refused_and_changes_nothing() {
    let mut world = world();
    let material = material(&mut world);
    let kept = buffer(&mut world, 1);
    world
        .set_material_buffer(material, "_Properties".into(), Some(kept))
        .unwrap();
    let before = world.material(material).unwrap().clone();
    let gone = buffer(&mut world, 2);
    world.destroy_compute_buffer(gone).unwrap();
    let refused = world.set_material_buffer(material, "_Properties".into(), Some(gone));
    assert_eq!(graphics_error(refused), GraphicsError::StaleComputeBuffer);
    let unknown = ComputeBufferId::from_bits(0x0000_0009_0000_0009);
    let refused = world.set_material_buffer(material, "_Other".into(), Some(unknown));
    assert_eq!(graphics_error(refused), GraphicsError::StaleComputeBuffer);
    assert_eq!(*world.material(material).unwrap(), before);
}

#[test]
fn a_binding_needs_a_live_material_and_a_name() {
    let mut world = world();
    let material = material(&mut world);
    let live = buffer(&mut world, 1);
    let refused = world.set_material_buffer(material, String::new(), Some(live));
    assert_eq!(graphics_error(refused), GraphicsError::InvalidName);
    let refused = world.set_material_buffer(material, "x".repeat(257), Some(live));
    assert_eq!(graphics_error(refused), GraphicsError::InvalidName);
    world.destroy_material(material).unwrap();
    let refused = world.set_material_buffer(material, "_Properties".into(), Some(live));
    assert_eq!(graphics_error(refused), GraphicsError::StaleMaterial);
}

#[test]
fn binding_no_buffer_removes_the_binding_and_clearing_properties_removes_them_all() {
    let mut world = world();
    let material = material(&mut world);
    let live = buffer(&mut world, 1);
    for name in ["_A", "_B"] {
        world
            .set_material_buffer(material, name.into(), Some(live))
            .unwrap();
    }
    world
        .set_material_buffer(material, "_A".into(), None)
        .unwrap();
    assert_eq!(
        world.material(material).unwrap().buffers,
        vec![("_B".to_owned(), live)]
    );
    world
        .edit_material(material, MaterialEdit::ClearProperties)
        .unwrap();
    assert!(world.material(material).unwrap().buffers.is_empty());
}

#[test]
fn a_snapshot_after_a_draw_carries_the_bound_buffer_once() {
    let mut world = world();
    let material = material(&mut world);
    let bound = buffer(&mut world, 0x5a);
    world
        .set_material_buffer(material, "_Properties".into(), Some(bound))
        .unwrap();
    world.record_offscreen(draw_mesh(material, 3)).unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(
        snapshot.materials[&material].buffers,
        vec![("_Properties".to_owned(), bound)]
    );
    assert_eq!(snapshot.compute_buffers.len(), 1);
    assert_eq!(snapshot.compute_buffers[&bound].contents, vec![0x5a; 16]);
}

#[test]
fn a_buffer_used_by_several_kinds_of_record_is_copied_once() {
    let mut world = world();
    let (mesh_material, procedural_material) = (material(&mut world), material(&mut world));
    let bound = buffer(&mut world, 7);
    for id in [mesh_material, procedural_material] {
        world
            .set_material_buffer(id, "_Properties".into(), Some(bound))
            .unwrap();
    }
    world.record_offscreen(draw_mesh(mesh_material, 2)).unwrap();
    world
        .record_offscreen(draw_procedural(procedural_material))
        .unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert_eq!(snapshot.compute_buffers.len(), 1);
    assert_eq!(snapshot.compute_buffers[&bound].contents, vec![7; 16]);
}

#[test]
fn a_destroyed_buffer_is_left_out_and_a_material_nothing_draws_copies_none() {
    let mut world = world();
    let (drawn, idle) = (material(&mut world), material(&mut world));
    let (gone, kept) = (buffer(&mut world, 1), buffer(&mut world, 2));
    world
        .set_material_buffer(drawn, "_Gone".into(), Some(gone))
        .unwrap();
    world
        .set_material_buffer(idle, "_Idle".into(), Some(kept))
        .unwrap();
    world.destroy_compute_buffer(gone).unwrap();
    world.record_offscreen(draw_mesh(drawn, 1)).unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    assert!(snapshot.compute_buffers.is_empty());
    assert_eq!(
        snapshot.materials[&drawn].buffers,
        vec![("_Gone".to_owned(), gone)],
        "the record keeps the stale binding for the host to skip"
    );
    assert!(!snapshot.materials.contains_key(&idle));
}

#[test]
fn the_wire_carries_material_buffers_with_their_contents() {
    let mut world = world();
    let material = material(&mut world);
    let bound = buffer(&mut world, 0x33);
    world
        .set_material_buffer(material, "_Properties".into(), Some(bound))
        .unwrap();
    world.record_offscreen(draw_mesh(material, 5)).unwrap();
    let snapshot = GraphicsSnapshot::take(&mut world).unwrap();
    let back = decode(&encode(&snapshot).unwrap()).unwrap();
    assert_eq!(back, snapshot);
    assert_eq!(back.materials[&material].buffers.len(), 1);
    assert_eq!(back.compute_buffers[&bound].contents, vec![0x33; 16]);
}
