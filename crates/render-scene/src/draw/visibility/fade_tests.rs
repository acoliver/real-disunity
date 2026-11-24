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
            mesh.fade_empty_alone(
                &instances.fades,
                source.geometry().objects()[0],
                1,
                Vec3::new(0.0, -7.0, 0.5)
            ),
            other == faded
        );
    }
}

/// A mesh of two levels, the first drawing `slots` materials, with no handoff.
fn mesh_drawing(slots: &[u32]) -> SceneMesh {
    let near = super::tests::level(0);
    let near = if slots.is_empty() {
        near
    } else {
        near.with_material_slots(
            slots
                .iter()
                .map(|&material| {
                    SceneLodMaterialSlot::new(MaterialHandle::from_raw(material), 0..3).unwrap()
                })
                .collect(),
        )
        .unwrap()
    };
    SceneMesh::with_distances(vec![near, super::tests::level(1)], vec![1000, 20_000]).unwrap()
}

#[test]
fn a_placement_is_not_faded_out_unless_a_band_of_a_material_it_draws_or_its_own_reaches_it() {
    let base = scene();
    let band = SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade();
    let atlas = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
        .unwrap()
        .with_fade(band);
    let live = base.resources().materials()[0];
    let faded = live.with_impostor(atlas);
    let mut reached = [false; 2];
    for (materials, slots, own_band, expected) in [
        ([live, live], &[][..], false, false),
        ([live, live], &[1][..], false, false),
        ([live, live], &[][..], true, true),
        ([live, live], &[1][..], true, true),
        ([live, faded], &[1][..], false, true),
        ([live, faded], &[0][..], false, false),
        ([faded, live], &[][..], false, true),
        ([faded, live], &[1][..], false, true),
    ] {
        let mesh = mesh_drawing(slots);
        let object = SceneObject::new(
            transform([0, 0, 500]),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        );
        let object = if own_band {
            object.with_fade(band)
        } else {
            object
        };
        let source = RenderScene::new(SceneParts {
            resources: SceneResources::new(
                base.resources().textures().to_vec(),
                materials.to_vec(),
            )
            .unwrap(),
            geometry: SceneGeometry::new(vec![mesh.clone()], vec![object]).unwrap(),
            terrain: None,
            effects: None,
        })
        .unwrap();
        let instances = Instances::new(&source);
        let selection = MeshSelection::new(&mesh);
        let reach = selection
            .slot_fade_reach(&instances.fades)
            .with(instances.fades[0]);
        let applies = reach.applies(object.fade().map(crate::geometry::fade_band_meters));
        assert_eq!(applies, expected, "{slots:?} {own_band}");
        reached[usize::from(applies)] = true;
        if !applies {
            for lod in 0..2 {
                for distance in [0.5_f32, 3.0, 4.0, 5.0, 7.0, 30.0] {
                    assert!(!selection.fade_empty_alone(
                        &instances.fades,
                        object,
                        lod,
                        Vec3::new(0.0, -distance, 0.5)
                    ));
                }
            }
        }
    }
    assert_eq!(reached, [true; 2]);
}
