//! The pass a material activated for the immediate-mode draws that follow.
use worldrunner_world_simulation::graphics::{ActivePass, GraphicsError, MaterialId, WorldShader};
use worldrunner_world_simulation::{World, WorldError, WorldSettings};

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn material(world: &mut World, name: &str) -> MaterialId {
    let shader = world
        .create_shader(WorldShader {
            name: name.into(),
            source: None,
        })
        .unwrap();
    world.create_material(shader).unwrap()
}

#[test]
fn a_world_starts_without_an_active_pass() {
    assert_eq!(world().active_pass(), None);
}

#[test]
fn setting_a_pass_makes_it_the_active_pass() {
    let mut world = world();
    let material = material(&mut world, "Icons");
    world.set_active_pass(material, 1).unwrap();
    assert_eq!(world.active_pass(), Some(ActivePass { material, pass: 1 }));
}

#[test]
fn a_second_pass_replaces_the_first() {
    let mut world = world();
    let first = material(&mut world, "A");
    let second = material(&mut world, "B");
    world.set_active_pass(first, 0).unwrap();
    world.set_active_pass(second, 2).unwrap();
    assert_eq!(
        world.active_pass(),
        Some(ActivePass {
            material: second,
            pass: 2
        })
    );
    world.set_active_pass(second, 0).unwrap();
    assert_eq!(
        world.active_pass(),
        Some(ActivePass {
            material: second,
            pass: 0
        })
    );
}

#[test]
fn an_unknown_material_is_refused_and_leaves_the_active_pass() {
    let mut world = world();
    let held = material(&mut world, "A");
    world.set_active_pass(held, 3).unwrap();
    let result = world.set_active_pass(MaterialId::from_bits(0), 0);
    assert!(matches!(
        result,
        Err(WorldError::Graphics(GraphicsError::StaleMaterial))
    ));
    assert_eq!(
        world.active_pass(),
        Some(ActivePass {
            material: held,
            pass: 3
        })
    );
}

#[test]
fn a_destroyed_material_is_refused() {
    let mut world = world();
    let gone = material(&mut world, "A");
    world.destroy_material(gone).unwrap();
    assert!(matches!(
        world.set_active_pass(gone, 0),
        Err(WorldError::Graphics(GraphicsError::StaleMaterial))
    ));
    assert_eq!(world.active_pass(), None);
}

#[test]
fn destroying_the_active_material_clears_the_active_pass() {
    let mut world = world();
    let active = material(&mut world, "A");
    world.set_active_pass(active, 1).unwrap();
    world.destroy_material(active).unwrap();
    assert_eq!(world.active_pass(), None);
}

#[test]
fn destroying_another_material_keeps_the_active_pass() {
    let mut world = world();
    let active = material(&mut world, "A");
    let other = material(&mut world, "B");
    world.set_active_pass(active, 1).unwrap();
    world.destroy_material(other).unwrap();
    assert_eq!(
        world.active_pass(),
        Some(ActivePass {
            material: active,
            pass: 1
        })
    );
}
