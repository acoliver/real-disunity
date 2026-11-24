//! Edits after which a warm slot's stored level inputs and level memo are wrong unless they
//! are cleared: placements turned or scaled about their origins while the eye stays still,
//! and a mesh replaced with other material-slot fade references and level handoffs at the
//! same level count and relief scales. After each edit, selection from the same eye must
//! equal the old fade code's scan and the selection of a scene built with the edit from the
//! start.
use worldrunner_render_plan::{
    BillboardPolicy, SceneCard, SceneFadeDirection, SceneGeometry, SceneLodMaterialSlot, SceneMesh,
    SceneMeshLod, SceneParts, SceneResources, SceneRotation,
};

use super::super::large_scene::{atlas, band, level, reference, slot};
use super::super::tests::scene;
use super::*;

/// Rotations about +z as `(z, w)` quaternion parts in millionths: none, an eighth, a quarter
/// and a half turn.
const TURNS: [(i64, i64); 4] = [
    (0, 1_000_000),
    (382_683, 923_880),
    (707_107, 707_107),
    (1_000_000, 0),
];
const SIZE: [i64; 3] = [1_500, 1_500, 4_000];
/// The mesh whose levels draw through material slots.
const SLOTTED: usize = 1;

/// Material 0 never fades. Materials 1 to 6 fade over bands between 25 and 65 metres, all but
/// one measured from authored references metres away from the placement's origin. Materials
/// 1 and 5 share a reference and differ in band.
fn materials() -> Vec<SceneMaterial> {
    let base = scene().resources().materials()[0];
    vec![
        base,
        base.with_impostor(atlas(band(40, 8, true)))
            .with_fade_reference(reference([9_000, 0, 1_500], None)),
        base.with_impostor(atlas(band(30, 10, false)))
            .with_fade_direction(SceneFadeDirection::Keep)
            .with_fade_reference(reference([-7_000, 4_000, 2_000], Some(10))),
        base.with_impostor(atlas(band(50, 10, false)))
            .with_fade_direction(SceneFadeDirection::Complement)
            .with_fade_reference(reference([0, 6_000, 500], None)),
        base.with_impostor(atlas(band(25, 10, true))),
        base.with_impostor(atlas(band(55, 10, true)))
            .with_fade_reference(reference([9_000, 0, 1_500], None)),
        base.with_impostor(atlas(band(35, 10, true)))
            .with_fade_reference(reference([3_000, -8_000, 0], Some(0))),
    ]
}

/// A yaw-locked card level, which the level before it hands off to.
fn card(index: u8) -> SceneMeshLod {
    let plain = level(index, SIZE);
    SceneMeshLod::new(
        index,
        plain.vertices().to_vec(),
        plain.indices().to_vec(),
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::YawLocked, [1_500, 4_000], 500_000).unwrap())
    .unwrap()
}

/// Three levels switching at 20, 45 and 200 metres, each drawing its two halves with the
/// materials `slots[level]`; `cards[level]` makes that level a card.
fn slotted(slots: [[u32; 2]; 3], cards: [bool; 3]) -> SceneMesh {
    let levels = (0..3_u8)
        .map(|index| {
            let plain = if cards[usize::from(index)] {
                card(index)
            } else {
                level(index, SIZE)
            };
            let [first, second] = slots[usize::from(index)];
            let slots: Vec<SceneLodMaterialSlot> = vec![slot(first, 0..3), slot(second, 3..6)];
            plain.with_material_slots(slots).unwrap()
        })
        .collect();
    SceneMesh::with_distances(levels, vec![20_000, 45_000, 200_000]).unwrap()
}

/// 600 placements 6 to 96 metres east of the origin, within the camera's view from there,
/// turned and scaled, half on a one-level mesh drawn with a fading material and half on the
/// slotted mesh; a third have fade bands of their own.
fn objects() -> Vec<SceneObject> {
    let mut rng = Rng::new(17);
    (0..600)
        .map(|_| {
            let east = rng.millimeters(6_000, 90_000);
            let side = u32::try_from(east * 4 / 5).unwrap();
            let (z, w) = TURNS[rng.index(TURNS.len())];
            let transform = SceneTransform {
                position_millimeters: [
                    east,
                    rng.millimeters(-east * 2 / 5, side),
                    rng.millimeters(0, 2_000),
                ],
                rotation: SceneRotation { x: 0, y: 0, z, w },
                scale_millionths: 600_000 + u64::from(rng.below(1_200_000)),
            };
            let slotted = rng.chance(50);
            let material = if slotted {
                rng.below(2) * 4
            } else {
                1 + rng.below(6)
            };
            let object = SceneObject::new(
                transform,
                MeshHandle::from_raw(u32::from(slotted)),
                MaterialHandle::from_raw(material),
            )
            .with_casts_shadow(rng.chance(85));
            if rng.chance(30) {
                object.with_fade(band(20 + rng.below(60), 3 + rng.below(8), rng.chance(70)))
            } else {
                object
            }
        })
        .collect()
}

/// The one-level mesh and `slotted`, placed by `objects`.
fn scene_with(slotted: SceneMesh, objects: Vec<SceneObject>) -> RenderScene {
    let single = SceneMesh::with_distances(vec![level(0, SIZE)], vec![400_000]).unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(scene().resources().textures().to_vec(), materials())
            .unwrap(),
        geometry: SceneGeometry::new(vec![single, slotted], objects).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

type Summary = (Vec<(BatchKey, usize, std::ops::Range<u32>)>, ViewStats);

/// Selects every view of `frame` from `warm` with the meshes of `fresh`, checks each view
/// against the old fade code's scan and against a scene built from `fresh`, which holds the
/// same placements in the same order, and returns the selections.
fn assert_matches_fresh(warm: &Instances, fresh: &RenderScene, frame: &FrameViews) -> Vec<Summary> {
    let meshes: Vec<MeshSelection> = fresh
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let summaries = |instances: &Instances| -> Vec<Summary> {
        select_frame(&RefCell::default(), instances, &meshes, frame, None)
            .iter()
            .map(|(_, _, selection)| summary(selection))
            .collect()
    };
    let selected = summaries(warm);
    assert_eq!(
        selected,
        summaries(&Instances::new(fresh)),
        "warm and fresh at {:?}",
        frame.eye_millimeters
    );
    let fades = reference_fades(fresh);
    assert_frame_matches_scan(&RefCell::default(), (warm, &fades), &meshes, frame, None);
    selected
}

/// The camera at two metres over the origin, looking east over the placements.
fn still_frame() -> FrameViews {
    views(Vec3::new(0.0, 0.0, 2.0), 0.0, -0.05)
}

#[test]
fn turning_or_scaling_warm_placements_about_their_origins_selects_like_a_fresh_scene() {
    let mesh = slotted([[1, 2], [3, 6], [5, 4]], [false, false, true]);
    let mut objects = objects();
    let mut warm = Instances::new(&scene_with(mesh.clone(), objects.clone()));
    let frame = still_frame();
    let before = assert_matches_fresh(&warm, &scene_with(mesh.clone(), objects.clone()), &frame);
    let handles: Vec<InstanceHandle> = warm.handles().collect();
    let mut rng = Rng::new(23);
    for (index, (&handle, object)) in handles.iter().zip(&mut objects).enumerate() {
        let mut transform = *object.transform();
        if index % 4 != 1 {
            let turned = loop {
                let (z, w) = TURNS[rng.index(TURNS.len())];
                if (z, w) != (transform.rotation.z, transform.rotation.w) {
                    break SceneRotation { x: 0, y: 0, z, w };
                }
            };
            transform.rotation = turned;
        }
        if index % 4 != 0 {
            transform.scale_millionths = if index % 2 == 0 {
                transform.scale_millionths * 3 / 2
            } else {
                transform.scale_millionths * 2 / 3
            };
        }
        // Every placement keeps its origin, so its distance from the eye, its level and
        // everything measured from its origin stay as they were; only its bounds and the
        // fades measured from an authored reference move.
        warm.set_transform(handle, transform).unwrap();
        *object = object.with_transform(transform);
    }
    let fresh = scene_with(mesh, objects);
    let after = assert_matches_fresh(&warm, &fresh, &frame);
    assert_ne!(
        before[0], after[0],
        "the edits change what the camera draws"
    );
    assert_matches_fresh(&warm, &fresh, &views(Vec3::new(3.0, -1.0, 2.0), 0.1, -0.05));
}

#[test]
fn replacing_slot_fade_references_and_handoffs_selects_like_a_fresh_scene() {
    let objects = objects();
    let mut warm = Instances::new(&scene_with(
        slotted([[1, 2], [3, 6], [5, 4]], [false, false, true]),
        objects.clone(),
    ));
    let frame = still_frame();
    let mut previous = assert_matches_fresh(
        &warm,
        &scene_with(
            slotted([[1, 2], [3, 6], [5, 4]], [false, false, true]),
            objects.clone(),
        ),
        &frame,
    );
    // First the slots draw other bands from the same references in the same order, so each
    // slot's stored anchors stay right, and the last level stops being a card, so nothing
    // hands off to it. Then the references themselves change and the handoff returns.
    for replacement in [
        slotted([[5, 2], [3, 6], [1, 4]], [false; 3]),
        slotted([[6, 2], [1, 4], [5, 4]], [false, false, true]),
    ] {
        // What replacing the mesh's contents does to the placements that draw it.
        warm.forget_mesh_bounds(SLOTTED);
        let selected =
            assert_matches_fresh(&warm, &scene_with(replacement, objects.clone()), &frame);
        assert_ne!(
            previous[0], selected[0],
            "the replacement changes what the camera draws"
        );
        previous = selected;
    }
}
