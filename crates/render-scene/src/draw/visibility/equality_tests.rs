//! The tree walk, the shared level memo and the caster fit against a full scan of every slot.
use glam::{Mat4, Vec3};
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneObject, SceneTransform};

use super::casters::caster_near;
use super::large_scene::{FrameViews, Rng, slice_hull, views, world};
use super::placement::{self, BatchShape};
use super::*;
use crate::instances::InstanceHandle;
use crate::instances::fade::{Distances, Fade};

/// Every live placement of every batch in slot order, with bounds computed afresh and each
/// material's fade measured on its own: the selection the tree walk must equal.
fn scan(instances: &Instances, meshes: &[MeshSelection], matrix: Mat4, view: &View) -> Selection {
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
                && (!view.casters_only || object.casts_shadow())
                && frustum.intersects(bounds)
                && view
                    .slice
                    .as_ref()
                    .is_none_or(|slice| slice.overlaps(bounds, matrix));
            let position = object.transform().position_millimeters;
            let lod = mesh.lod(
                distance_squared(position, view.eye_millimeters),
                object.lod_distance_scale_millionths(),
            );
            if !admitted
                || (!instances.fade_cull_disabled
                    && fade_empty_alone(mesh, &instances.fades, object, lod, view.fade_eye))
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
    instances: &Instances,
    meshes: &[MeshSelection],
    frame: &FrameViews,
    terrain: Option<Bounds>,
) {
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
        |placements| {
            placements.select_each(
                &mut selections
                    .iter_mut()
                    .map(|(matrix, selection)| (*matrix, selection))
                    .collect::<Vec<_>>(),
            );
        },
    );
    for (index, ((matrix, view), (_, selected))) in views.iter().zip(&selections).enumerate() {
        assert_eq!(
            summary(selected),
            summary(&scan(instances, meshes, *matrix, view)),
            "view {index} at {:?}",
            frame.eye_millimeters
        );
    }
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

/// Moves, hides, removes and adds placements, as streaming and gameplay do.
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
        match rng.below(4) {
            0 => instances.set_transform(handle, transform).unwrap(),
            1 => instances.set_visible(handle, rng.chance(50)).unwrap(),
            2 => instances.remove(handle).unwrap(),
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

#[test]
fn tree_walk_shared_levels_and_caster_fit_equal_the_full_scan() {
    let source = world(5_000, 24, 11);
    let mut instances = Instances::new(&source);
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
        for _ in 0..4 {
            let frame = random_frame(&mut rng);
            let terrain = (round % 2 == 0).then_some(terrain);
            assert_frame_matches_scan(&instances, &meshes, &frame, terrain);
        }
        // The same eye again reuses the level memo filled above.
        let frame = super::large_scene::walk(round * 7);
        assert_frame_matches_scan(&instances, &meshes, &frame, None);
        assert_frame_matches_scan(&instances, &meshes, &frame, None);
        instances.fade_cull_disabled = round == 3;
        edit(&mut instances, &mut rng, &mut added);
    }
    assert!(!added.is_empty());
}
