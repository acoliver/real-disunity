//! The tree walk, the shared level memo, the cached level inputs, the fade estimate and the
//! caster fit against a full scan of every slot with the level and fade tests as they were
//! before those caches.
use std::cell::RefCell;

use glam::{Mat4, Vec3};
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, RenderScene, SceneFadeBand, SceneImpostor, SceneMaterial,
    SceneObject, SceneTransform,
};

use super::super::slice_hull::reference as hull_reference;
use super::casters::caster_near;
use super::frustum::reference as frustum_reference;
use super::large_scene::{FrameViews, Rng, slice_hull, views, world};
use super::placement::{self, BatchShape};
use super::*;
use crate::instances::InstanceHandle;
use crate::instances::fade::reference::{Distances, Fade};

/// The fade test of every material as it was before anchors and estimates.
fn reference_fades(source: &RenderScene) -> Vec<Fade> {
    let textures = source.resources().textures();
    source
        .resources()
        .materials()
        .iter()
        .map(|material| Fade::new(material, textures))
        .collect()
}

/// The level choice as it was before scaled ceilings.
fn reference_lod(mesh: &MeshSelection, distance: u128, scale: Option<u32>) -> usize {
    mesh.ceilings_squared
        .iter()
        .position(|&ceiling| within_ceiling(distance, ceiling, scale))
        .unwrap_or_else(|| mesh.ceilings_squared.len().saturating_sub(1))
}

/// Every live placement of every batch in slot order, with bounds computed afresh, each
/// material's fade measured on its own and the reference level and fade tests: the selection
/// the tree walk must equal.
fn scan(
    (instances, fades): (&Instances, &[Fade]),
    meshes: &[MeshSelection],
    matrix: Mat4,
    view: &View,
) -> Selection {
    let mut selection = Selection {
        casters_only: view.casters_only,
        slice: view.slice.clone(),
        ..Selection::default()
    };
    selection.reset(meshes);
    let frustum = Frustum::new(matrix);
    let stretch = if instances.impostor_extents.iter().any(|e| e[1] != 0.0) {
        relief_ray_stretch(matrix)
    } else {
        1.0
    };
    for (&key, batch) in &instances.batches {
        let mesh = &meshes[key.1];
        let shape = BatchShape::new(mesh, instances, key.2);
        let mut boxes = vec![[Vec3::ZERO; 2]; shape.reliefs().len()];
        for (slot, object, visible) in batch.placements() {
            selection.stats.considered += 1;
            shape.fill(mesh, instances, &object, &mut boxes);
            let bounds = placement::placement(&boxes, shape.reliefs(), stretch);
            let admitted = visible
                && if view.casters_only {
                    object.casts_shadow()
                } else {
                    !object.shadow_only()
                }
                && frustum_reference::intersects(&frustum, bounds)
                && view
                    .slice
                    .as_ref()
                    .is_none_or(|slice| hull_reference::overlaps(slice, bounds, matrix));
            let position = object.transform().position_millimeters;
            let lod = reference_lod(
                mesh,
                distance_squared(position, view.eye_millimeters),
                object.lod_distance_scale_millionths(),
            );
            if !admitted
                || (!instances.fade_cull_disabled
                    && fade_empty_alone(mesh, fades, object, lod, view.fade_eye))
            {
                selection.stats.culled += 1;
                continue;
            }
            selection.push(key, lod, slot);
        }
    }
    selection
}

fn fade_empty_alone(
    mesh: &MeshSelection,
    fades: &[Fade],
    object: SceneObject,
    lod: usize,
    eye: Vec3,
) -> bool {
    let alone =
        |material: u32| fades[material as usize].empty(object, eye, &mut Distances::default());
    let level = |lod: usize| match &mesh.material_slots[lod] {
        Some(slots) => slots.iter().all(|slot| alone(slot.material().raw())),
        None => alone(object.material().raw()),
    };
    level(lod) && (!mesh.transitions[lod] || level(lod - 1))
}

/// The caster fit as a scan of every caster in batch and slot order.
fn scan_casters(
    instances: &Instances,
    meshes: &[MeshSelection],
    terrain: Option<Bounds>,
    receiver: Mat4,
) -> Mat4 {
    let mut near = terrain.map_or(0.0, |bounds| caster_near(bounds, receiver));
    for (&(_, mesh, material), batch) in &instances.batches {
        let shape = BatchShape::new(&meshes[mesh], instances, material);
        let mut boxes = vec![[Vec3::ZERO; 2]; shape.reliefs().len()];
        for (_, object, visible) in batch.placements() {
            if visible && object.casts_shadow() {
                shape.fill(&meshes[mesh], instances, &object, &mut boxes);
                let bounds = placement::placement(&boxes, shape.reliefs(), 0.0);
                near = near.min(caster_near(bounds, receiver));
            }
        }
    }
    let scale = (1.0 - near).recip();
    let mut remap = Mat4::IDENTITY;
    remap.z_axis.z = scale;
    remap.w_axis.z = -near * scale;
    remap * receiver
}

struct View {
    casters_only: bool,
    slice: Option<super::super::slice_hull::SliceHull>,
    eye_millimeters: [i64; 3],
    fade_eye: Vec3,
}

fn summary(selection: &Selection) -> (Vec<(BatchKey, usize, std::ops::Range<u32>)>, ViewStats) {
    (
        selection
            .ranges
            .iter()
            .map(|range| (range.key, range.lod, range.slots.clone()))
            .collect(),
        selection.stats.clone(),
    )
}

fn bits(matrix: Mat4) -> [u32; 16] {
    matrix.to_cols_array().map(f32::to_bits)
}

/// The views of one frame: the camera with and without the caster filter, and each cascade
/// with its fitted matrix, with and without its slice.
fn frame_views(
    instances: &Instances,
    meshes: &[MeshSelection],
    frame: &FrameViews,
    terrain: Option<Bounds>,
) -> Vec<(Mat4, View)> {
    let view = |casters_only, slice| View {
        casters_only,
        slice,
        eye_millimeters: frame.eye_millimeters,
        fade_eye: frame.fade_eye,
    };
    let mut views = vec![
        (frame.camera, view(false, None)),
        (frame.camera, view(true, None)),
    ];
    for (index, &receiver) in frame.receivers.iter().enumerate() {
        let fit = caster_matrix(instances, meshes, terrain, receiver);
        assert_eq!(
            bits(fit),
            bits(scan_casters(instances, meshes, terrain, receiver)),
            "caster fit of cascade {index}"
        );
        views.push((fit, view(true, Some(slice_hull(frame, index, fit)))));
        views.push((fit, view(true, None)));
    }
    views
}

/// Selects every view of `frame` together, in parallel as the renderer does, and compares
/// each with the scan.
fn assert_frame_matches_scan(
    rows: &RefCell<PlacementRows>,
    (instances, fades): (&Instances, &[Fade]),
    meshes: &[MeshSelection],
    frame: &FrameViews,
    terrain: Option<Bounds>,
) {
    let selections = select_frame(rows, instances, meshes, frame, terrain);
    for (index, (matrix, view, selected)) in selections.iter().enumerate() {
        assert_eq!(
            summary(selected),
            summary(&scan((instances, fades), meshes, *matrix, view)),
            "view {index} at {:?}",
            frame.eye_millimeters
        );
    }
}

/// The views of `frame`, each with its matrix and its selection, every view selected
/// together, in parallel as the renderer does.
fn select_frame(
    rows: &RefCell<PlacementRows>,
    instances: &Instances,
    meshes: &[MeshSelection],
    frame: &FrameViews,
    terrain: Option<Bounds>,
) -> Vec<(Mat4, View, Selection)> {
    let views = frame_views(instances, meshes, frame, terrain);
    let mut selections: Vec<(Mat4, Selection)> = views
        .iter()
        .map(|(matrix, view)| {
            let selection = Selection {
                casters_only: view.casters_only,
                slice: view.slice.clone(),
                ..Selection::default()
            };
            (*matrix, selection)
        })
        .collect();
    with_placements(
        instances,
        meshes,
        (frame.eye_millimeters, frame.fade_eye),
        rows,
        |placements| {
            placements.select_each(
                &mut selections
                    .iter_mut()
                    .map(|(matrix, selection)| (*matrix, selection))
                    .collect::<Vec<_>>(),
                4,
            );
        },
    );
    views
        .into_iter()
        .zip(selections)
        .map(|((matrix, view), (_, selection))| (matrix, view, selection))
        .collect()
}

/// Selects the camera with and without the caster filter, sharing the level memo, and
/// compares both with the scan.
fn assert_camera_matches_scan(
    (instances, fades): (&Instances, &[Fade]),
    meshes: &[MeshSelection],
    frame: &FrameViews,
) {
    let view = |casters_only| View {
        casters_only,
        slice: None,
        eye_millimeters: frame.eye_millimeters,
        fade_eye: frame.fade_eye,
    };
    with_placements(
        instances,
        meshes,
        (frame.eye_millimeters, frame.fade_eye),
        &std::cell::RefCell::default(),
        |placements| {
            for casters_only in [false, true] {
                let mut selection = Selection {
                    casters_only,
                    ..Selection::default()
                };
                placements.select_each(&mut [(frame.camera, &mut selection)], 1);
                assert_eq!(
                    summary(&selection),
                    summary(&scan(
                        (instances, fades),
                        meshes,
                        frame.camera,
                        &view(casters_only)
                    )),
                    "camera at {:?} {:?}",
                    frame.eye_millimeters,
                    frame.fade_eye
                );
            }
        },
    );
}

fn random_frame(rng: &mut Rng) -> FrameViews {
    views(
        Vec3::new(
            rng.range(-1300.0, 1300.0),
            rng.range(-1300.0, 1300.0),
            rng.range(0.5, 60.0),
        ),
        rng.range(0.0, std::f32::consts::TAU),
        rng.range(-0.8, 0.4),
    )
}

/// Moves, hides, removes, adds and refades placements, as streaming and gameplay do.
fn edit(instances: &mut Instances, rng: &mut Rng, added: &mut Vec<InstanceHandle>) {
    let handles: Vec<InstanceHandle> = instances.handles().collect();
    for _ in 0..handles.len() / 20 {
        let handle = handles[rng.index(handles.len())];
        let Ok((object, _)) = instances.get(handle) else {
            continue;
        };
        let mut transform: SceneTransform = *object.transform();
        transform.position_millimeters[0] += rng.millimeters(-80_000, 160_000);
        transform.position_millimeters[2] += rng.millimeters(-5_000, 10_000);
        match rng.below(6) {
            0 => instances.set_transform(handle, transform).unwrap(),
            1 => instances.set_visible(handle, rng.chance(50)).unwrap(),
            2 => instances.remove(handle).unwrap(),
            3 => {
                let start = 30_000 + rng.below(300_000);
                let band = SceneFadeBand::new(start, start + rng.below(40_000)).unwrap();
                instances
                    .set_object(handle, object.with_fade(band))
                    .unwrap();
            }
            4 => {
                let scale = 300_000 + rng.below(2_000_000);
                let object = object.with_lod_distance_scale_millionths(scale).unwrap();
                instances.set_object(handle, object).unwrap();
            }
            _ => added.push(
                instances
                    .add(
                        SceneObject::new(
                            transform,
                            MeshHandle::from_raw(rng.below(4)),
                            MaterialHandle::from_raw(rng.below(8)),
                        ),
                        rng.chance(90),
                    )
                    .unwrap(),
            ),
        }
    }
}

/// A camera at `eye` metres, exact millimetres `eye_millimeters`, looking towards `target`.
fn looking(eye: Vec3, eye_millimeters: [i64; 3], target: Vec3) -> FrameViews {
    let toward = target - eye;
    let mut frame = views(eye, toward.y.atan2(toward.x), -0.05);
    frame.eye_millimeters = eye_millimeters;
    frame
}

/// Frames whose eye sits on either side of placements' level ceilings, to the millimetre,
/// and around the edges of the fade bands their materials measure, from each material's own
/// reference point, by a unit in the last place of the eye's coordinates.
fn crossing_frames(
    instances: &Instances,
    (meshes, materials): (&[MeshSelection], &[SceneMaterial]),
    rng: &mut Rng,
) -> Vec<FrameViews> {
    let mut frames = Vec::new();
    let handles: Vec<InstanceHandle> = instances.handles().collect();
    for _ in 0..2 {
        let (object, _) = instances.get(handles[rng.index(handles.len())]).unwrap();
        let position = object.transform().position_millimeters;
        let origin = crate::geometry::millimeters_to_meters(position);
        let mesh = &meshes[object.mesh().raw() as usize];
        let scale = u128::from(object.lod_distance_scale_millionths().unwrap_or(1_000_000));
        for &ceiling in &mesh.ceilings_squared {
            // The ceilings are squared whole millimetres; the scaled boundary is near this.
            let reach = i64::try_from(ceiling.isqrt() * scale / 1_000_000).unwrap();
            for offset in [-1, 0, 1] {
                let eye_millimeters = [position[0] + reach + offset, position[1], position[2]];
                let eye = crate::geometry::millimeters_to_meters(eye_millimeters);
                frames.push(looking(eye, eye_millimeters, origin));
            }
        }
        let drawn: Vec<usize> = std::iter::once(object.material().raw() as usize)
            .chain(
                mesh.material_slots
                    .iter()
                    .flatten()
                    .flatten()
                    .map(|slot| slot.material().raw() as usize),
            )
            .collect();
        for _ in 0..3 {
            let material = materials[drawn[rng.index(drawn.len())]];
            let Some(band) = object
                .fade()
                .or(material.impostor().and_then(SceneImpostor::fade))
            else {
                continue;
            };
            let center = material.fade_reference().map_or(origin, |reference| {
                crate::instances::fade::Anchor::referenced(object.transform(), reference).middle()
            });
            let direction = Vec3::new(rng.range(-1.0, 1.0), rng.range(-1.0, 1.0), 0.1);
            for edge in crate::geometry::fade_band_meters(band).map(f32::abs) {
                let step = (center.abs().max_element() + edge).next_up()
                    - (center.abs().max_element() + edge);
                for offset in [-1.0, 0.0, 1.0] {
                    let eye = center + direction.normalize() * (edge + offset * step);
                    let eye_millimeters = (eye * 1000.0).round().as_i64vec3().to_array();
                    frames.push(looking(eye, eye_millimeters, center));
                }
            }
        }
    }
    frames
}

#[test]
fn tree_walk_shared_levels_and_caster_fit_equal_the_full_scan() {
    let source = world(5_000, 24, 11);
    let fades = reference_fades(&source);
    let materials = source.resources().materials();
    let mut instances = Instances::new(&source);
    let rows = RefCell::default();
    assert!(instances.has_skinned_batches());
    // Relief, wind and curved wind make bounds depend on the material and the view.
    instances.impostor_extents[3][1] = 0.75;
    instances.impostor_extents[6] = [0.4, 1.5, 0.2];
    instances.wind_extents[2] = Vec3::new(0.5, 0.25, 0.1);
    instances.curved_wind[4] = Some(Vec3::new(0.0, 0.0, 0.3));
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let mut rng = Rng::new(5);
    let mut added = Vec::new();
    let terrain = Bounds {
        min: Vec3::new(-1200.0, -1200.0, -20.0),
        max: Vec3::new(1200.0, 1200.0, 45.0),
    };
    for round in 0..6_u16 {
        let both = (&instances, fades.as_slice());
        for _ in 0..4 {
            let frame = random_frame(&mut rng);
            let terrain = (round % 2 == 0).then_some(terrain);
            assert_frame_matches_scan(&rows, both, &meshes, &frame, terrain);
        }
        // The same eye again reuses the level memo filled above.
        let frame = super::large_scene::walk(round * 7);
        assert_frame_matches_scan(&rows, both, &meshes, &frame, None);
        assert_frame_matches_scan(&rows, both, &meshes, &frame, None);
        // A walk in steps under a millimetre and over one, so the memo empties every frame.
        for step in 0..6_u16 {
            let eye = Vec3::new(-300.0, -200.0, 2.0)
                + Vec3::new(0.0007, 0.0013, 0.0) * f32::from(step * (round + 1));
            assert_frame_matches_scan(&rows, both, &meshes, &views(eye, 0.4, -0.05), None);
        }
        for frame in crossing_frames(&instances, (&meshes, materials), &mut rng) {
            assert_camera_matches_scan(both, &meshes, &frame);
        }
        instances.fade_cull_disabled = round == 3;
        edit(&mut instances, &mut rng, &mut added);
    }
    assert!(!added.is_empty());
}

/// Makes about one placement in `every` shadow-only, and turns back about as many, so the
/// cameras and the cascades disagree about them. A non-casting placement may be shadow-only
/// too: it then draws in neither view.
fn toggle_shadow_only(instances: &mut Instances, rng: &mut Rng, every: usize) {
    let handles: Vec<InstanceHandle> = instances.handles().collect();
    for _ in 0..handles.len() / every {
        let handle = handles[rng.index(handles.len())];
        let Ok((object, _)) = instances.get(handle) else {
            continue;
        };
        let shadow_only = !object.shadow_only();
        instances
            .set_object(handle, object.with_shadow_only(shadow_only))
            .unwrap();
    }
}

/// Shadow-only placements leave the camera views and stay in the cascades, in the full scan
/// and in the tree walk alike, as placements turn shadow-only and back between frames.
#[test]
fn shadow_only_placements_leave_the_cameras_alike_in_the_walk_and_the_full_scan() {
    let source = world(3_000, 24, 31);
    let fades = reference_fades(&source);
    let mut instances = Instances::new(&source);
    let rows = RefCell::default();
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let mut rng = Rng::new(31);
    let mut added = Vec::new();
    toggle_shadow_only(&mut instances, &mut rng, 4);
    let shadow_only = || {
        instances
            .handles()
            .filter(|&handle| {
                instances
                    .get(handle)
                    .is_ok_and(|(object, _)| object.shadow_only())
            })
            .count()
    };
    assert!(shadow_only() > 0);
    for round in 0..4_u16 {
        let both = (&instances, fades.as_slice());
        for _ in 0..3 {
            assert_frame_matches_scan(&rows, both, &meshes, &random_frame(&mut rng), None);
        }
        let frame = super::large_scene::walk(round * 9);
        assert_frame_matches_scan(&rows, both, &meshes, &frame, None);
        toggle_shadow_only(&mut instances, &mut rng, 10);
        edit(&mut instances, &mut rng, &mut added);
    }
}

/// The slot counts of every leaf of every batch's tree.
fn leaf_sizes(instances: &Instances) -> std::collections::BTreeSet<usize> {
    use crate::instances::slot_bounds::Node;
    let mut sizes = std::collections::BTreeSet::new();
    for batch in instances.batches.values() {
        let bounds = batch.bounds.borrow();
        let tree = bounds.tree();
        let mut stack: Vec<usize> = tree.root().into_iter().collect();
        while let Some(index) = stack.pop() {
            match tree.node(index) {
                Node::Inner { children, .. } => stack.extend(children),
                Node::Leaf { slots, .. } => {
                    sizes.insert(slots.len());
                }
            }
        }
    }
    sizes
}

/// Worlds whose batches hold a few placements each, as a scene of many small prop batches
/// does, so the leaves on both sides of the size below which a view tests slots one by one
/// are selected like the full scan, with and without edits.
#[test]
fn leaves_of_every_size_around_the_one_by_one_threshold_equal_the_full_scan() {
    let mut seen = std::collections::BTreeSet::new();
    for (placements, seed) in [(60, 21), (150, 22), (400, 23), (900, 24)] {
        let source = world(placements, 24, seed);
        let fades = reference_fades(&source);
        let meshes: Vec<_> = source
            .geometry()
            .meshes()
            .iter()
            .map(MeshSelection::new)
            .collect();
        let mut instances = Instances::new(&source);
        let rows = RefCell::default();
        let mut rng = Rng::new(seed);
        let mut added = Vec::new();
        for round in 0..3_u16 {
            for step in 0..5_u16 {
                let frame = if step < 3 {
                    super::large_scene::walk(round * 11 + step * 5)
                } else {
                    random_frame(&mut rng)
                };
                assert_frame_matches_scan(&rows, (&instances, &fades), &meshes, &frame, None);
            }
            seen.extend(leaf_sizes(&instances));
            edit(&mut instances, &mut rng, &mut added);
        }
    }
    assert!(
        (1..=select::SMALL_LEAF + 1).all(|size| seen.contains(&size)),
        "leaf sizes seen: {seen:?}"
    );
}

mod edit_tests;

/// Every view of hundreds of successive eyes, more than the level memos' epochs count, with the
/// batch rows and the memos kept from frame to frame and placements edited between them, equals
/// the full scan.
#[test]
fn rows_and_level_memos_kept_across_many_eyes_and_edits_equal_the_full_scan() {
    let source = world(700, 24, 31);
    let fades = reference_fades(&source);
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let mut instances = Instances::new(&source);
    let rows = RefCell::default();
    let mut rng = Rng::new(31);
    let mut added = Vec::new();
    for step in 0..600_u16 {
        // Eyes a millimetre apart, so each frame is a new eye of one neighbourhood.
        let eye = Vec3::new(-250.0, -150.0, 2.0) + Vec3::new(0.001, 0.0007, 0.0) * f32::from(step);
        assert_frame_matches_scan(
            &rows,
            (&instances, &fades),
            &meshes,
            &views(eye, 0.4, -0.05),
            None,
        );
        if step % 97 == 96 {
            edit(&mut instances, &mut rng, &mut added);
        }
    }
}

/// Every view of eyes that stride past the levels' distance ceilings and fade bands, return to
/// earlier places and jump across the world, with placements edited between them, equals the
/// full scan, so a level kept for the eye it was computed at is never kept beyond its reach.
#[test]
fn levels_kept_while_the_eye_strides_past_ceilings_and_jumps_equal_the_full_scan() {
    let source = world(700, 24, 41);
    let fades = reference_fades(&source);
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let mut instances = Instances::new(&source);
    let rows = RefCell::default();
    let mut rng = Rng::new(41);
    let mut added = Vec::new();
    let mut eye = Vec3::new(-300.0, -150.0, 2.0);
    let mut visited = vec![eye];
    for step in 0..360_u16 {
        eye = match step % 47 {
            46 => Vec3::new(
                rng.range(-1300.0, 1300.0),
                rng.range(-1300.0, 1300.0),
                rng.range(0.5, 60.0),
            ),
            23 => visited[rng.index(visited.len())],
            _ => {
                eye + Vec3::new(
                    rng.range(-1.0, 6.0),
                    rng.range(-2.0, 2.0),
                    rng.range(-0.1, 0.1),
                )
            }
        };
        visited.push(eye);
        assert_frame_matches_scan(
            &rows,
            (&instances, &fades),
            &meshes,
            &views(eye, 0.4, -0.05),
            None,
        );
        if step % 89 == 88 {
            edit(&mut instances, &mut rng, &mut added);
        }
    }
}
