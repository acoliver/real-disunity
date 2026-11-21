//! Caster fits from one scan shared by every cascade of a frame equal a slot-by-slot scan of
//! the placements, bit for bit, while the camera walks and while placements are edited.
use super::casters::{CasterScan, FitScratch, caster_near};
use super::large_scene::{FrameViews, walk, world};
use super::*;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneObject, SceneTransform,
};

fn meshes_of(source: &RenderScene) -> Vec<MeshSelection> {
    source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect()
}

fn bits(matrix: Mat4) -> [u32; 16] {
    matrix.to_cols_array().map(f32::to_bits)
}

/// The caster fit as the definition states it: the nearest reach of every visible caster in
/// batch and slot order, with no search tree and nothing reused between calls.
fn slot_scan(
    instances: &Instances,
    meshes: &[MeshSelection],
    terrain: Option<Bounds>,
    receiver: Mat4,
) -> Mat4 {
    placement::refresh_all(instances, meshes);
    let start = terrain.map_or(0.0, |bounds| caster_near(bounds, receiver));
    let near = instances.batches.values().fold(start, |near, batch| {
        let bounds = batch.bounds.borrow();
        batch
            .slots()
            .iter()
            .enumerate()
            .filter_map(|(slot, placement)| {
                let (object, visible) = (*placement)?;
                (visible && object.casts_shadow()).then(|| {
                    caster_near(
                        placement::placement(bounds.boxes(slot), bounds.reliefs(), 0.0),
                        receiver,
                    )
                })
            })
            .fold(near, f32::min)
    });
    let scale = (1.0 - near).recip();
    let mut remap = Mat4::IDENTITY;
    remap.z_axis.z = scale;
    remap.w_axis.z = -near * scale;
    remap * receiver
}

/// One frame's fits through a single scan, in the order the cascades ask for them.
fn scanned(
    (instances, meshes): (&Instances, &[MeshSelection]),
    terrain: Option<Bounds>,
    receivers: &[Mat4],
    scratch: &mut FitScratch,
) -> Vec<Mat4> {
    let mut scan = CasterScan::new(instances, meshes, scratch);
    receivers
        .iter()
        .map(|&receiver| scan.matrix(terrain, receiver))
        .collect()
}

/// Asserts that the scan and the slot scan agree on every receiver, and returns how many fits
/// moved the near plane.
fn assert_frame(
    world: (&Instances, &[MeshSelection]),
    terrain: Option<Bounds>,
    receivers: &[Mat4],
    scratch: &mut FitScratch,
    context: &str,
) -> usize {
    let fits = scanned(world, terrain, receivers, scratch);
    assert_eq!(fits.len(), receivers.len(), "{context}");
    for (index, (&fit, &receiver)) in fits.iter().zip(receivers).enumerate() {
        let expected = slot_scan(world.0, world.1, terrain, receiver);
        assert_eq!(bits(fit), bits(expected), "{context}: cascade {index}");
    }
    fits.iter()
        .zip(receivers)
        .filter(|(fit, receiver)| bits(**fit) != bits(**receiver))
        .count()
}

/// The frame's cascade receivers with each near plane moved to the middle of the cascade's
/// slice, so casters above the slice reach toward the light beyond it. A walking camera's own
/// receivers start far above the world, where no caster reaches.
fn receivers_of(views: &FrameViews) -> Vec<Mat4> {
    views
        .receivers
        .iter()
        .zip(&views.slices)
        .map(|(&receiver, corners)| {
            let center = corners.iter().copied().sum::<Vec3>() / 8.0;
            let depth = receiver.transform_point3(center).z;
            Mat4::from_translation(Vec3::new(0.0, 0.0, -depth)) * receiver
        })
        .collect()
}

#[test]
fn one_scan_fits_every_cascade_of_a_walking_camera_like_a_slot_scan() {
    let source = world(1_500, 20, 5);
    let instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let mut extended = 0;
    for step in 0..12 {
        extended += assert_frame(
            (&instances, &meshes),
            None,
            &receivers_of(&walk(step)),
            &mut scratch,
            &format!("frame {step}"),
        );
    }
    assert!(
        extended > 0,
        "some cascade reaches casters beyond its near plane"
    );
}

#[test]
fn a_cascade_asked_twice_in_one_scan_and_a_terrain_start_fit_like_a_slot_scan() {
    let source = world(800, 12, 11);
    let instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let terrain = Bounds {
        min: Vec3::new(-1_200.0, -1_200.0, -40.0),
        max: Vec3::new(1_200.0, 1_200.0, 40.0),
    };
    let mut receivers = receivers_of(&walk(3));
    receivers.extend(receivers_of(&walk(3)));
    let mut extended = 0;
    for terrain in [None, Some(terrain)] {
        extended += assert_frame(
            (&instances, &meshes),
            terrain,
            &receivers,
            &mut scratch,
            "repeated receivers",
        );
    }
    assert!(extended > 0, "some cascade reaches beyond its near plane");
}

#[test]
fn the_number_of_cascades_a_frame_fits_changes_nothing() {
    let source = world(800, 12, 3);
    let instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let mut extended = 0;
    for (step, count) in [(0, 4), (1, 2), (2, 3), (3, 1), (4, 4), (5, 0), (6, 4)] {
        let mut receivers = receivers_of(&walk(step));
        receivers.truncate(count);
        extended += assert_frame(
            (&instances, &meshes),
            None,
            &receivers,
            &mut scratch,
            &format!("frame {step} with {count} cascades"),
        );
    }
    assert!(extended > 0, "some cascade reaches beyond its near plane");
}

fn moved(object: SceneObject, position_millimeters: [i64; 3]) -> SceneObject {
    object.with_transform(SceneTransform {
        position_millimeters,
        ..*object.transform()
    })
}

/// A caster 80 m above the middle of the second cascade's slice at the start of the walk.
fn upstream(like: SceneObject) -> SceneObject {
    let corners = walk(0).slices[1];
    let center = corners.iter().copied().sum::<Vec3>() / 8.0;
    let millimeters = (center + Vec3::new(0.0, 0.0, 80.0)) * 1_000.0;
    moved(
        like.with_casts_shadow(true),
        millimeters.round().as_i64vec3().to_array(),
    )
}

#[test]
fn every_edit_of_a_placement_reaches_the_next_scan() {
    let source = world(1_200, 16, 9);
    let mut instances = Instances::new(&source);
    let mut meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let mut frame = 0_u16;
    let mut check = |instances: &Instances, meshes: &[MeshSelection], context: &str| {
        frame += 1;
        let extended = assert_frame(
            (instances, meshes),
            None,
            &receivers_of(&walk(frame)),
            &mut scratch,
            context,
        );
        assert!(
            extended > 0,
            "{context}: a cascade reaches beyond its near plane"
        );
    };
    check(&instances, &meshes, "unedited");
    let handles: Vec<_> = instances.handles().collect();
    let (first, second, third) = (handles[3], handles[40], handles[200]);

    let (object, _) = instances.get(first).unwrap();
    instances.set_object(first, upstream(object)).unwrap();
    check(&instances, &meshes, "moved");
    instances
        .set_object(first, moved(object, [900_000, 900_000, 0]))
        .unwrap();
    check(&instances, &meshes, "moved back out of reach");

    let (object, _) = instances.get(second).unwrap();
    instances.set_visible(second, false).unwrap();
    check(&instances, &meshes, "hidden");
    instances.set_object(second, upstream(object)).unwrap();
    check(&instances, &meshes, "hidden and moved");
    instances.set_visible(second, true).unwrap();
    check(&instances, &meshes, "shown");
    instances
        .set_object(second, upstream(object).with_casts_shadow(false))
        .unwrap();
    check(&instances, &meshes, "stopped casting");

    // A placement moves to another batch when its material or mesh changes.
    let (object, _) = instances.get(third).unwrap();
    let remapped = SceneObject::new(
        *object.transform(),
        object.mesh(),
        MaterialHandle::from_raw((object.material().raw() + 1) % 8),
    );
    instances.remove(third).unwrap();
    let third = instances.add(upstream(remapped), true).unwrap();
    check(&instances, &meshes, "material changed");
    let rebuilt = SceneObject::new(
        *object.transform(),
        MeshHandle::from_raw((object.mesh().raw() + 1) % 16),
        remapped.material(),
    );
    instances.remove(third).unwrap();
    instances.add(upstream(rebuilt), true).unwrap();
    check(&instances, &meshes, "mesh changed");

    let added = instances.add(upstream(object), true).unwrap();
    check(&instances, &meshes, "added");
    instances.remove(added).unwrap();
    check(&instances, &meshes, "removed");
    instances.remove(first).unwrap();
    check(&instances, &meshes, "removed an edited placement");

    meshes[2] = MeshSelection::new(&source.geometry().meshes()[5]);
    instances.forget_mesh_bounds(2);
    check(&instances, &meshes, "mesh bounds replaced");
}

#[test]
fn a_batch_that_grows_is_fitted_over_all_its_placements() {
    let source = world(300, 6, 21);
    let mut instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let template = instances
        .get(instances.handles().next().unwrap())
        .unwrap()
        .0;
    for (round, extra) in [1_usize, 7, 64, 300].into_iter().enumerate() {
        let batches = instances.batches.len();
        for index in 0..extra {
            let offset = i64::try_from(index).unwrap() * 1_500;
            let position = [
                -200_000 + offset,
                -150_000 + offset / 3,
                150_000 + offset / 2,
            ];
            instances.add(moved(template, position), true).unwrap();
        }
        let extended = assert_frame(
            (&instances, &meshes),
            None,
            &receivers_of(&walk(2 * u16::try_from(round).unwrap())),
            &mut scratch,
            &format!("round {round}"),
        );
        assert!(
            extended > 0,
            "round {round}: a cascade reaches beyond its near plane"
        );
        assert_eq!(instances.batches.len(), batches, "the batch itself grew");
    }
}

#[test]
fn a_new_batch_joins_the_next_scan() {
    let source = world(300, 6, 2);
    let mut instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let receivers = receivers_of(&walk(4));
    let before = scanned((&instances, &meshes), None, &receivers, &mut scratch);
    let batches = instances.batches.len();
    let template = instances
        .get(instances.handles().next().unwrap())
        .unwrap()
        .0;
    let mut made = false;
    for material in 0..8 {
        let object = SceneObject::new(
            *template.transform(),
            MeshHandle::from_raw(5),
            MaterialHandle::from_raw(material),
        );
        instances.add(upstream(object), true).unwrap();
        made |= instances.batches.len() > batches;
    }
    assert!(made, "a placement made a batch the scan had not seen");
    let after = scanned((&instances, &meshes), None, &receivers, &mut scratch);
    assert_ne!(
        before.iter().copied().map(bits).collect::<Vec<_>>(),
        after.iter().copied().map(bits).collect::<Vec<_>>(),
        "the new caster reaches toward the light"
    );
    assert_frame(
        (&instances, &meshes),
        None,
        &receivers,
        &mut scratch,
        "new batch",
    );
}

#[test]
fn scratch_capacity_carries_no_state_between_worlds() {
    let mut scratch = FitScratch::default();
    let mut extended = 0;
    for seed in [31, 32, 33] {
        let source = world(600, 10, seed);
        let instances = Instances::new(&source);
        let meshes = meshes_of(&source);
        extended += assert_frame(
            (&instances, &meshes),
            None,
            &receivers_of(&walk(u16::try_from(seed).unwrap())),
            &mut scratch,
            &format!("world {seed}"),
        );
    }
    assert!(extended > 0, "some cascade reaches beyond its near plane");
}

#[test]
fn receivers_that_no_caster_reaches_fit_to_themselves_like_a_slot_scan() {
    let source = world(1_500, 20, 17);
    let instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    for step in 0..6 {
        let views = walk(step);
        let receivers = views.receivers.to_vec();
        let extended = assert_frame(
            (&instances, &meshes),
            None,
            &receivers,
            &mut scratch,
            &format!("frame {step}"),
        );
        assert_eq!(
            extended, 0,
            "the walk's own receivers start above the world"
        );
    }
}

#[test]
fn a_terrain_start_that_no_caster_undercuts_decides_the_fit_like_a_slot_scan() {
    let source = world(1_500, 20, 19);
    let instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let tall = Bounds {
        min: Vec3::new(-1_500.0, -1_500.0, -10.0),
        max: Vec3::new(1_500.0, 1_500.0, 5_000.0),
    };
    let mut extended = 0;
    for step in 0..4 {
        for receivers in [receivers_of(&walk(step)), walk(step).receivers.to_vec()] {
            extended += assert_frame(
                (&instances, &meshes),
                Some(tall),
                &receivers,
                &mut scratch,
                &format!("frame {step}"),
            );
        }
    }
    assert!(extended > 0, "the terrain reaches toward the light");
}

#[test]
fn a_world_without_placements_fits_every_receiver_to_itself() {
    let source = world(200, 6, 23);
    let mut instances = Instances::new(&source);
    let meshes = meshes_of(&source);
    let mut scratch = FitScratch::default();
    let receivers = receivers_of(&walk(2));
    let handles: Vec<_> = instances.handles().collect();
    let before = scanned((&instances, &meshes), None, &receivers, &mut scratch);
    assert_ne!(
        before.iter().copied().map(bits).collect::<Vec<_>>(),
        receivers.iter().copied().map(bits).collect::<Vec<_>>(),
        "the placements reach toward the light"
    );
    for handle in handles {
        instances.remove(handle).unwrap();
    }
    let extended = assert_frame(
        (&instances, &meshes),
        None,
        &receivers,
        &mut scratch,
        "all removed",
    );
    assert_eq!(extended, 0);
}
