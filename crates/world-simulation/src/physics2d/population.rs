//! How many 2D physics components the world stores.
//!
//! A step reads every component several times to find bodies and colliders. A scene with none,
//! which includes most scenes that only draw, has nothing to simulate, so the step needs no scan.
//! The world counts bodies and colliders as they are stored and retired. Components pending
//! destruction still count, so the count can only err toward running the full step.

use super::Contacts;
use crate::{ComponentDesc, World};
use std::collections::BTreeMap;

/// Whether `desc` is something the 2D physics step reads.
pub(crate) fn is_physics(desc: &ComponentDesc) -> bool {
    matches!(
        desc,
        ComponentDesc::Body2d(_) | ComponentDesc::Collider2d(_)
    )
}

#[derive(Clone, Copy, Default)]
pub(crate) struct Population {
    stored: usize,
}

impl Population {
    pub(crate) fn note_stored(&mut self, desc: &ComponentDesc) {
        if is_physics(desc) {
            self.stored += 1;
        }
    }

    pub(crate) fn note_retired(&mut self, desc: &ComponentDesc) {
        if is_physics(desc) {
            self.stored = self.stored.saturating_sub(1);
        }
    }

    pub(crate) fn is_empty(self) -> bool {
        self.stored == 0
    }
}

impl World {
    /// What the full step publishes when it finds nothing to simulate: every contact that existed
    /// ends, and no body keeps interpolation poses.
    pub(super) fn step_nothing(&mut self) {
        self.finish_contacts(&Contacts::new(), Contacts::new());
        self.physics.step_poses = BTreeMap::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics2d::{Body2d, Collider2d};
    use crate::{DestroyWhen, ObjectDesc, WorldSettings};

    fn scanned(world: &World) -> usize {
        world
            .components
            .iter(world.identity)
            .filter(|(_, component)| is_physics(&component.desc))
            .count()
    }

    fn assert_counted(world: &World) {
        assert_eq!(world.physics_population.stored, scanned(world));
        assert_eq!(world.physics_population.is_empty(), scanned(world) == 0);
    }

    fn body() -> ComponentDesc {
        ComponentDesc::Body2d(Body2d::default())
    }

    fn collider() -> ComponentDesc {
        ComponentDesc::Collider2d(Collider2d::default())
    }

    #[test]
    fn only_bodies_and_colliders_are_counted() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        assert!(world.physics_population.is_empty());
        world
            .add_component(object, ComponentDesc::Renderer { material_slot: 0 })
            .unwrap();
        assert_counted(&world);
        assert!(world.physics_population.is_empty());
        let body = world.add_component(object, body()).unwrap();
        assert!(!world.physics_population.is_empty());
        world.add_component(object, collider()).unwrap();
        assert_counted(&world);
        world.remove_component(body).unwrap();
        assert_counted(&world);
        assert!(!world.physics_population.is_empty());
    }

    #[test]
    fn the_count_follows_clones_removal_and_retirement() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let template = world.create_object(ObjectDesc::default()).unwrap();
        world.add_component(template, body()).unwrap();
        world.add_component(template, collider()).unwrap();
        let clone = world.instantiate(template, None).unwrap();
        assert_counted(&world);
        assert_eq!(world.physics_population.stored, 4);
        world.destroy(clone.root, DestroyWhen::EndOfFrame).unwrap();
        assert_eq!(world.physics_population.stored, 4);
        world.finish_phase();
        assert_counted(&world);
        world.destroy_object(template).unwrap();
        assert_counted(&world);
        assert!(world.physics_population.is_empty());
    }

    #[test]
    fn rolling_back_a_staged_collider_restores_the_count() {
        let mut world = World::new(WorldSettings::default()).unwrap();
        let object = world.create_object(ObjectDesc::default()).unwrap();
        world.add_component(object, body()).unwrap();
        world.stage_for_test();
        world.add_component(object, collider()).unwrap();
        assert_eq!(world.physics_population.stored, 2);
        world.roll_back_for_test();
        assert_counted(&world);
        assert_eq!(world.physics_population.stored, 1);
    }
}
