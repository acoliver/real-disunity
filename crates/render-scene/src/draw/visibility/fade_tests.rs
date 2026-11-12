mod threshold_tests;

use super::tests::{scene, transform};
use super::*;
use worldrunner_render_plan::*;

fn fixture(material: &SceneMaterial, bands: &[Option<SceneFadeBand>]) -> RenderScene {
    let base = scene();
    let objects = bands
        .iter()
        .map(|band| {
            let object = SceneObject::new(
                transform([0, 0, 500]),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            );
            band.map_or(object, |band| object.with_fade(band))
        })
        .collect();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(base.resources().textures().to_vec(), vec![*material])
            .unwrap(),
        geometry: SceneGeometry::new(base.geometry().meshes().to_vec(), objects).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

#[test]
fn empty_terminal_fades_leave_camera_and_caster_selection_and_counts() {
    let base = scene();
    let band = SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade();
    let source = fixture(&base.resources().materials()[0], &[None, Some(band)]);
    let instances = Instances::new(&source);
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    for casters_only in [false, true] {
        let mut selection = Selection {
            casters_only,
            ..Selection::default()
        };
        selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0, -7000, 500]);
        assert_eq!(selection.stats.considered, 2);
        assert_eq!(selection.stats.culled, 1);
        assert_eq!(selection.stats.drawn_per_lod.iter().sum::<usize>(), 1);
        assert_eq!(selection.ranges.len(), 1);
        assert_eq!(selection.ranges[0].slots, 0..1);
    }
}

#[test]
fn fade_in_and_complement_use_the_shader_boundary_direction() {
    let base = scene();
    let band = SceneFadeBand::new(2000, 6000).unwrap();
    for direction in [
        SceneFadeDirection::Inclusive,
        SceneFadeDirection::Keep,
        SceneFadeDirection::Complement,
    ] {
        let material = base.resources().materials()[0].with_fade_direction(direction);
        let source = fixture(&material, &[Some(band)]);
        let instances = Instances::new(&source);
        let meshes: Vec<_> = source
            .geometry()
            .meshes()
            .iter()
            .map(MeshSelection::new)
            .collect();
        for (distance, culled) in [(1000, true), (4000, false), (7000, false)] {
            let mut selection = Selection::default();
            selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0, -distance, 500]);
            assert_eq!(
                selection.stats.culled,
                usize::from(culled),
                "{direction:?} at {distance}"
            );
        }
    }
}

#[test]
fn selection_uses_shader_eye_without_changing_integer_lod_selection() {
    let base = scene();
    let band = SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade();
    let source = fixture(&base.resources().materials()[0], &[Some(band)]);
    let instances = Instances::new(&source);
    let meshes: Vec<_> = source
        .geometry()
        .meshes()
        .iter()
        .map(MeshSelection::new)
        .collect();
    let mut selected = Selection::default();
    selected.rebuild_with_eye(
        &instances,
        &meshes,
        Mat4::IDENTITY,
        [0; 3],
        Vec3::new(0.0, -7.0, 0.5),
    );
    assert_eq!(selected.stats.culled, 1);
    selected.rebuild_with_eye(
        &instances,
        &meshes,
        Mat4::IDENTITY,
        [0; 3],
        Vec3::new(0.0, -1.0, 0.5),
    );
    assert_eq!(selected.stats.culled, 0);
    assert_eq!(selected.stats.drawn_per_lod, vec![1, 0, 0]);
}

#[test]
fn a_live_material_slot_or_mesh_handoff_prevents_whole_placement_culling() {
    let base = scene();
    let band = SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade();
    let atlas = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
        .unwrap()
        .with_fade(band);
    let live = base.resources().materials()[0];
    let faded = live.with_impostor(atlas);
    let near = super::tests::level(0)
        .with_material_slots(vec![
            SceneLodMaterialSlot::new(MaterialHandle::from_raw(1), 0..3).unwrap(),
        ])
        .unwrap();
    let far = SceneMeshLod::new(
        1,
        near.vertices().to_vec(),
        near.indices().to_vec(),
        Some(BillboardPolicy::YawLocked),
    )
    .unwrap()
    .with_card(SceneCard::new(BillboardPolicy::YawLocked, [1000, 1000], 500_000).unwrap())
    .unwrap();
    let mesh = SceneMesh::with_distances(vec![near, far], vec![1000, 20_000]).unwrap();
    for other in [live, faded] {
        let source = RenderScene::new(SceneParts {
            resources: SceneResources::new(
                base.resources().textures().to_vec(),
                vec![faded, other],
            )
            .unwrap(),
            geometry: SceneGeometry::new(
                vec![mesh.clone()],
                vec![SceneObject::new(
                    transform([0, 0, 500]),
                    MeshHandle::from_raw(0),
                    MaterialHandle::from_raw(0),
                )],
            )
            .unwrap(),
            terrain: None,
            effects: None,
        })
        .unwrap();
        let instances = Instances::new(&source);
        let mesh = MeshSelection::new(&mesh);
        assert_eq!(
            mesh.fade_empty(
                &instances,
                source.geometry().objects()[0],
                1,
                Vec3::new(0.0, -7.0, 0.5)
            ),
            other == faded
        );
    }
}
