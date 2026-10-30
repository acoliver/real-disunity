//! Spatial index behavior, checked against brute-force evaluation over the public world API.

use std::time::{Duration, Instant};

use glam::{Quat, Vec3};
use worldrunner_world_simulation::{
    DestroyWhen, LocalTransform, ObjectDesc, ObjectEdit, ObjectHandle, ParentSpace, World,
    WorldError, WorldSettings,
};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, bound: usize) -> usize {
        usize::try_from(self.next() % u64::try_from(bound).unwrap()).unwrap()
    }

    fn unit(&mut self) -> f32 {
        f32::from(u16::try_from(self.next() % 10_000).unwrap()) / 10_000.0
    }

    fn range(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }

    fn point(&mut self, extent: f32) -> Vec3 {
        Vec3::new(
            self.range(-extent, extent),
            self.range(-extent, extent),
            self.range(-extent, extent),
        )
    }
}

fn world() -> World {
    World::new(WorldSettings::default()).unwrap()
}

fn at(world: &mut World, position: Vec3) -> ObjectHandle {
    world
        .create_object(ObjectDesc {
            local_transform: LocalTransform {
                translation: position,
                ..LocalTransform::default()
            },
            ..ObjectDesc::default()
        })
        .unwrap()
}

fn live_positions(world: &World) -> Vec<(ObjectHandle, Vec3)> {
    let mut found: Vec<_> = world
        .snapshot()
        .objects
        .iter()
        .filter(|o| world.active_in_hierarchy(o.handle).unwrap())
        .map(|o| (o.handle, world.world_position(o.handle).unwrap()))
        .collect();
    found.sort_by_key(|(handle, _)| *handle);
    found
}

fn oracle_radius(all: &[(ObjectHandle, Vec3)], center: Vec3, radius: f32) -> Vec<ObjectHandle> {
    all.iter()
        .filter(|(_, p)| (*p - center).length_squared() <= radius * radius)
        .map(|(h, _)| *h)
        .collect()
}

fn oracle_box(all: &[(ObjectHandle, Vec3)], min: Vec3, max: Vec3) -> Vec<ObjectHandle> {
    all.iter()
        .filter(|(_, p)| p.cmpge(min).all() && p.cmple(max).all())
        .map(|(h, _)| *h)
        .collect()
}

fn oracle_nearest(
    all: &[(ObjectHandle, Vec3)],
    center: Vec3,
    max_radius: f32,
    keep: impl Fn(ObjectHandle) -> bool,
) -> Option<ObjectHandle> {
    let mut best: Option<(f32, ObjectHandle)> = None;
    for &(handle, p) in all {
        let d = (p - center).length_squared();
        if d > max_radius * max_radius || !keep(handle) {
            continue;
        }
        if best.is_none_or(|(bd, bh)| d < bd || (d == bd && handle < bh)) {
            best = Some((d, handle));
        }
    }
    best.map(|(_, h)| h)
}

fn assert_matches_oracle(world: &World, rng: &mut Rng, queries: usize) {
    let all = live_positions(world);
    for _ in 0..queries {
        let center = rng.point(120.0);
        let radius = rng.range(0.0, 40.0);
        assert_eq!(
            world.objects_in_radius(center, radius).unwrap(),
            oracle_radius(&all, center, radius)
        );
        let min = rng.point(120.0);
        let max = min
            + Vec3::new(
                rng.range(0.0, 60.0),
                rng.range(0.0, 60.0),
                rng.range(0.0, 60.0),
            );
        assert_eq!(
            world.objects_in_box(min, max).unwrap(),
            oracle_box(&all, min, max)
        );
        let max_radius = rng.range(0.0, 500.0);
        let skip = all.get(rng.below(all.len().max(1))).map(|(h, _)| *h);
        let keep = |h: ObjectHandle| Some(h) != skip;
        assert_eq!(
            world.nearest(center, max_radius, keep).unwrap(),
            oracle_nearest(&all, center, max_radius, keep)
        );
    }
}

fn random_local(rng: &mut Rng) -> LocalTransform {
    LocalTransform {
        translation: rng.point(100.0),
        rotation: Quat::from_rotation_y(rng.range(-3.0, 3.0)),
        scale: Vec3::splat(rng.range(0.5, 2.0)),
    }
}

fn mutate(world: &mut World, rng: &mut Rng, live: &[ObjectHandle]) {
    let object = live[rng.below(live.len())];
    // Operations on objects destroyed earlier in the batch fail and are skipped.
    match rng.below(10) {
        0..=3 => drop(world.set_local_transform(object, random_local(rng))),
        4 => drop(world.set_world_position(object, rng.point(100.0))),
        5 | 6 => {
            let parent = live[rng.below(live.len())];
            let space = if rng.below(2) == 0 {
                ParentSpace::KeepLocal
            } else {
                ParentSpace::KeepWorld
            };
            drop(world.set_parent(object, Some(parent), space));
        }
        7 => drop(world.set_parent(object, None, ParentSpace::KeepWorld)),
        8 => drop(world.set_active(object, rng.below(3) != 0)),
        _ => {
            if rng.below(4) == 0 {
                drop(world.destroy_object(object));
            }
        }
    }
}

fn populated(count: usize, rng: &mut Rng) -> (World, Vec<ObjectHandle>) {
    let mut world = world();
    let objects = (0..count)
        .map(|_| at(&mut world, rng.point(100.0)))
        .collect();
    (world, objects)
}

fn run_randomized(enable_first: bool) {
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    let (mut world, mut live) = populated(3000, &mut rng);
    if enable_first {
        world.enable_spatial_index(9.0).unwrap();
    }
    for batch in 0..12 {
        for _ in 0..250 {
            mutate(&mut world, &mut rng, &live);
        }
        if !enable_first && batch == 5 {
            world.enable_spatial_index(9.0).unwrap();
        }
        if world.has_spatial_index() {
            assert_matches_oracle(&world, &mut rng, 8);
        }
        live = world.snapshot().objects.iter().map(|o| o.handle).collect();
        for _ in 0..15 {
            let created = at(&mut world, rng.point(100.0));
            live.push(created);
        }
    }
}

#[test]
fn queries_match_brute_force_through_incremental_mutations() {
    run_randomized(true);
}

#[test]
fn enabling_midway_matches_brute_force() {
    run_randomized(false);
}

#[test]
fn refreshing_does_not_change_answers() {
    let mut rng = Rng(77);
    let (mut world, live) = populated(2000, &mut rng);
    world.enable_spatial_index(5.0).unwrap();
    for _ in 0..600 {
        mutate(&mut world, &mut rng, &live);
    }
    let center = Vec3::new(1.0, 2.0, 3.0);
    let before = world.objects_in_radius(center, 60.0).unwrap();
    world.refresh_spatial_index();
    assert_eq!(world.objects_in_radius(center, 60.0).unwrap(), before);
    assert_matches_oracle(&world, &mut rng, 20);
}

#[test]
fn results_follow_a_moved_parent_to_its_descendants() {
    let mut world = world();
    world.enable_spatial_index(2.0).unwrap();
    let parent = at(&mut world, Vec3::ZERO);
    let child = at(&mut world, Vec3::new(1.0, 0.0, 0.0));
    let grandchild = at(&mut world, Vec3::new(0.0, 1.0, 0.0));
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    world
        .set_parent(grandchild, Some(child), ParentSpace::KeepLocal)
        .unwrap();
    world.refresh_spatial_index();
    let far = Vec3::new(500.0, 0.0, 0.0);
    world
        .set_local_transform(
            parent,
            LocalTransform {
                translation: far,
                ..LocalTransform::default()
            },
        )
        .unwrap();
    assert_eq!(
        world.objects_in_radius(far, 3.0).unwrap(),
        vec![parent, child, grandchild]
    );
    assert!(world.objects_in_radius(Vec3::ZERO, 3.0).unwrap().is_empty());
    world.refresh_spatial_index();
    assert_eq!(
        world.objects_in_radius(far, 3.0).unwrap(),
        vec![parent, child, grandchild]
    );
}

#[test]
fn deactivating_an_ancestor_hides_descendants_until_reactivated() {
    let mut world = world();
    world.enable_spatial_index(4.0).unwrap();
    let parent = at(&mut world, Vec3::ZERO);
    let child = at(&mut world, Vec3::ONE);
    world
        .set_parent(child, Some(parent), ParentSpace::KeepLocal)
        .unwrap();
    world.set_active(parent, false).unwrap();
    assert!(
        world
            .objects_in_radius(Vec3::ZERO, 50.0)
            .unwrap()
            .is_empty()
    );
    world.refresh_spatial_index();
    assert!(
        world
            .objects_in_box(Vec3::splat(-9.0), Vec3::splat(9.0))
            .unwrap()
            .is_empty()
    );
    world.set_active(parent, true).unwrap();
    assert_eq!(world.objects_in_radius(Vec3::ZERO, 50.0).unwrap().len(), 2);
}

#[test]
fn destroyed_objects_leave_the_index_immediately_or_at_phase_end() {
    let mut world = world();
    world.enable_spatial_index(4.0).unwrap();
    let immediate = at(&mut world, Vec3::ZERO);
    let deferred = at(&mut world, Vec3::X);
    world.destroy_object(immediate).unwrap();
    assert_eq!(
        world.objects_in_radius(Vec3::ZERO, 5.0).unwrap(),
        vec![deferred]
    );
    world.destroy(deferred, DestroyWhen::EndOfFrame).unwrap();
    world.finish_phase();
    assert!(world.objects_in_radius(Vec3::ZERO, 5.0).unwrap().is_empty());
}

#[test]
fn failed_edit_batch_restores_indexed_positions() {
    let mut world = world();
    world.enable_spatial_index(4.0).unwrap();
    let object = at(&mut world, Vec3::ZERO);
    world.refresh_spatial_index();
    let edits = [ObjectEdit::Translation(object, Vec3::splat(300.0))];
    assert!(world.step_with_edits(&edits, -1.0).is_err());
    assert_eq!(
        world.objects_in_radius(Vec3::ZERO, 1.0).unwrap(),
        vec![object]
    );
    assert!(
        world
            .objects_in_radius(Vec3::splat(300.0), 1.0)
            .unwrap()
            .is_empty()
    );
    world.apply_edits(&edits).unwrap();
    assert_eq!(
        world.objects_in_radius(Vec3::splat(300.0), 1.0).unwrap(),
        vec![object]
    );
}

#[test]
fn radius_and_box_boundaries_are_inclusive() {
    let mut world = world();
    world.enable_spatial_index(2.0).unwrap();
    let on_x = at(&mut world, Vec3::new(3.0, 0.0, 0.0));
    let on_cell_edge = at(&mut world, Vec3::new(-2.0, 4.0, 0.0));
    let outside = at(&mut world, Vec3::new(3.001, 0.0, 0.0));
    assert_eq!(
        world.objects_in_radius(Vec3::ZERO, 3.0).unwrap(),
        vec![on_x]
    );
    assert_eq!(
        world
            .objects_in_radius(Vec3::new(3.0, 0.0, 0.0), 0.0)
            .unwrap(),
        vec![on_x]
    );
    assert_eq!(
        world
            .objects_in_box(Vec3::new(-2.0, 0.0, 0.0), Vec3::new(3.0, 4.0, 0.0))
            .unwrap(),
        vec![on_x, on_cell_edge]
    );
    assert!(
        !world
            .objects_in_box(Vec3::splat(-9.0), Vec3::splat(9.0))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        world
            .objects_in_box(Vec3::new(3.001, 0.0, 0.0), Vec3::new(3.001, 0.0, 0.0))
            .unwrap(),
        vec![outside]
    );
    assert!(
        world
            .objects_in_box(Vec3::X, Vec3::NEG_X)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn nearest_breaks_ties_by_lower_handle_and_applies_filter_and_limit() {
    let mut world = world();
    world.enable_spatial_index(3.0).unwrap();
    let reference = at(&mut world, Vec3::ZERO);
    let a = at(&mut world, Vec3::new(2.0, 0.0, 0.0));
    let b = at(&mut world, Vec3::new(-2.0, 0.0, 0.0));
    let far = at(&mut world, Vec3::new(40.0, 0.0, 0.0));
    let others = |h: ObjectHandle| h != reference;
    assert_eq!(world.nearest(Vec3::ZERO, 10.0, others).unwrap(), Some(a));
    assert_eq!(
        world
            .nearest(Vec3::ZERO, 10.0, |h| h != reference && h != a)
            .unwrap(),
        Some(b)
    );
    assert_eq!(world.nearest(Vec3::ZERO, 10.0, |h| h == far).unwrap(), None);
    assert_eq!(
        world.nearest(Vec3::ZERO, 40.0, |h| h == far).unwrap(),
        Some(far)
    );
    assert_eq!(
        world.nearest(Vec3::ZERO, 39.999, |h| h == far).unwrap(),
        None
    );
    assert_eq!(
        world.nearest(Vec3::ZERO, f32::MAX, |h| h == far).unwrap(),
        Some(far)
    );
}

#[test]
fn huge_query_extents_stay_correct() {
    let mut world = world();
    world.enable_spatial_index(1.0).unwrap();
    let object = at(&mut world, Vec3::new(1.0e6, -2.0e6, 3.0e6));
    assert_eq!(
        world
            .objects_in_box(Vec3::splat(-f32::MAX), Vec3::splat(f32::MAX))
            .unwrap(),
        vec![object]
    );
    assert_eq!(
        world.objects_in_radius(Vec3::ZERO, f32::MAX).unwrap(),
        vec![object]
    );
}

#[test]
fn results_are_in_ascending_handle_order_whatever_the_cell_layout() {
    let mut rng = Rng(9);
    let (mut world, _) = populated(1500, &mut rng);
    world.enable_spatial_index(3.0).unwrap();
    let found = world.objects_in_radius(Vec3::ZERO, 80.0).unwrap();
    assert!(found.windows(2).all(|pair| pair[0] < pair[1]));
    let in_box = world
        .objects_in_box(Vec3::splat(-80.0), Vec3::splat(80.0))
        .unwrap();
    assert!(in_box.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn queries_fail_until_the_index_is_enabled() {
    let mut world = world();
    at(&mut world, Vec3::ZERO);
    assert!(!world.has_spatial_index());
    assert_eq!(
        world.objects_in_radius(Vec3::ZERO, 1.0),
        Err(WorldError::SpatialIndexDisabled)
    );
    assert_eq!(
        world.objects_in_box(Vec3::ZERO, Vec3::ONE),
        Err(WorldError::SpatialIndexDisabled)
    );
    assert_eq!(
        world.nearest(Vec3::ZERO, 1.0, |_| true),
        Err(WorldError::SpatialIndexDisabled)
    );
    world.enable_spatial_index(1.0).unwrap();
    assert_eq!(world.objects_in_radius(Vec3::ZERO, 1.0).unwrap().len(), 1);
}

#[test]
fn invalid_arguments_are_rejected() {
    let mut world = world();
    for size in [0.0, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(
            world.enable_spatial_index(size),
            Err(WorldError::InvalidSpatialArgument)
        );
    }
    world.enable_spatial_index(1.0).unwrap();
    let bad = WorldError::InvalidSpatialArgument;
    assert_eq!(world.objects_in_radius(Vec3::ZERO, -1.0), Err(bad.clone()));
    assert_eq!(
        world.objects_in_radius(Vec3::ZERO, f32::NAN),
        Err(bad.clone())
    );
    assert_eq!(world.objects_in_radius(Vec3::NAN, 1.0), Err(bad.clone()));
    assert_eq!(world.objects_in_box(Vec3::NAN, Vec3::ONE), Err(bad.clone()));
    assert_eq!(world.nearest(Vec3::ZERO, -1.0, |_| true), Err(bad));
}

#[test]
fn changing_the_cell_size_rebuilds_with_the_same_answers() {
    let mut rng = Rng(31);
    let (mut world, live) = populated(800, &mut rng);
    world.enable_spatial_index(10.0).unwrap();
    for _ in 0..200 {
        mutate(&mut world, &mut rng, &live);
    }
    let before = world.objects_in_radius(Vec3::ZERO, 70.0).unwrap();
    world.enable_spatial_index(2.5).unwrap();
    assert_eq!(world.objects_in_radius(Vec3::ZERO, 70.0).unwrap(), before);
    assert_matches_oracle(&world, &mut rng, 10);
}

#[test]
fn many_objects_answer_many_queries_within_a_loose_bound() {
    let mut rng = Rng(2024);
    let (mut world, _) = populated(20_000, &mut rng);
    world.enable_spatial_index(10.0).unwrap();
    let started = Instant::now();
    let mut matched = 0;
    for _ in 0..5_000 {
        matched += world
            .objects_in_radius(rng.point(100.0), 8.0)
            .unwrap()
            .len();
    }
    assert!(matched > 0);
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "5000 radius queries over 20000 objects took {:?}",
        started.elapsed()
    );
}
