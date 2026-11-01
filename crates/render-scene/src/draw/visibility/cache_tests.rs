use super::placement::BatchBounds;
use super::tests::{level, mesh, scene, transform};
use super::*;
use worldrunner_render_plan::*;

#[derive(Clone, Copy, Default)]
struct Material {
    wind: Vec3,
    curved: Option<Vec3>,
    /// Shaped radius, relief scale and capture radius, as stored by `Instances`.
    impostor: [f32; 3],
}

fn instances(materials: &[Material]) -> Instances {
    let mut instances = Instances::new(&scene());
    instances.wind_extents.clear();
    instances.curved_wind.clear();
    instances.impostor_extents.clear();
    for material in materials {
        instances.wind_extents.push(material.wind);
        instances.curved_wind.push(material.curved);
        instances.impostor_extents.push(material.impostor);
    }
    instances
}

fn object(placement: SceneTransform) -> SceneObject {
    SceneObject::new(
        placement,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
}

fn placements() -> [SceneTransform; 3] {
    let mut turned = transform([1_234, -567, 8_901]);
    turned.rotation = SceneRotation {
        x: 0,
        y: 382_683,
        z: 0,
        w: 923_880,
    };
    turned.scale_millionths = 2_500_000;
    let mut small = transform([-40_000, 3_000, 0]);
    small.rotation = SceneRotation {
        x: 0,
        y: 0,
        z: 707_107,
        w: 707_107,
    };
    small.scale_millionths = 300_000;
    [transform([0; 3]), turned, small]
}

fn bits(bounds: Bounds) -> [[u32; 3]; 2] {
    [bounds.min, bounds.max].map(|corner| corner.to_array().map(f32::to_bits))
}

/// Cached bounds must equal the uncached reference bit for bit, for views with and
/// without relief stretch, both when the slot is filled and when it is read back.
fn assert_matches_reference(mesh: &SceneMesh, materials: &[Material]) {
    let selection = MeshSelection::new(mesh);
    let mut instances = instances(materials);
    for placement in placements() {
        let handle = instances.add(object(placement), true).unwrap();
        let (&key, batch) = instances.batches.iter().next().unwrap();
        let (slot, _, _) = batch.placements().next().unwrap();
        let mut cached = BatchBounds::new(&selection, &instances, key.2, batch);
        for stretch in [1.37, 0.0, 1.0, 1.37] {
            assert_eq!(
                bits(cached.placement(slot, &placement, stretch)),
                bits(selection.placement_bounds(&instances, &placement, 0, stretch)),
                "stretch {stretch}"
            );
        }
        drop(cached);
        instances.remove(handle).unwrap();
    }
}

fn billboard() -> SceneMesh {
    let vertex = |x| SceneVertex {
        tangent_micro: None,
        position_millimeters: [x, 150, -40],
        normal_micro: [0, 0, 1_000_000],
        texcoord_micro: [0; 2],
    };
    let lod = SceneMeshLod::new(
        0,
        vec![vertex(-300), vertex(500)],
        vec![0, 1, 0],
        Some(BillboardPolicy::Spherical),
    )
    .unwrap();
    SceneMesh::new(vec![lod]).unwrap()
}

#[test]
fn plain_and_billboard_bounds_match_uncached_reference() {
    assert_matches_reference(&mesh(), &[Material::default()]);
    assert_matches_reference(&billboard(), &[Material::default()]);
}

#[test]
fn wind_and_shaped_wind_bounds_match_uncached_reference() {
    let wind = Vec3::new(0.3, 0.1, 0.2);
    assert_matches_reference(
        &mesh(),
        &[Material {
            wind,
            ..Material::default()
        }],
    );
    assert_matches_reference(
        &billboard(),
        &[Material {
            wind,
            impostor: [1.5, 0.0, 0.0],
            ..Material::default()
        }],
    );
}

#[test]
fn curved_wind_bounds_match_uncached_reference() {
    let curved = Material {
        wind: Vec3::new(0.05, 0.0, 0.4),
        curved: Some(Vec3::new(0.2, 0.5, 0.1)),
        impostor: [0.5, 0.0, 0.3],
    };
    assert_matches_reference(&mesh(), &[curved]);
    assert_matches_reference(&billboard(), &[curved]);
}

#[test]
fn impostor_relief_bounds_match_uncached_reference() {
    assert_matches_reference(
        &billboard(),
        &[Material {
            impostor: [0.0, 2.0, 0.0],
            ..Material::default()
        }],
    );
    assert_matches_reference(
        &billboard(),
        &[Material {
            impostor: [0.7, 2.0, 0.4],
            ..Material::default()
        }],
    );
}

#[test]
fn slotted_mesh_bounds_match_uncached_reference_across_relief_groups() {
    let slots = |materials: &[u32]| {
        (0_u32..)
            .zip(materials)
            .map(|(index, &material)| {
                let range = index * 3..index * 3 + 3;
                SceneLodMaterialSlot::new(MaterialHandle::from_raw(material), range).unwrap()
            })
            .collect::<Vec<_>>()
    };
    let near = level(0);
    let two_triangles =
        SceneMeshLod::new(0, near.vertices().to_vec(), vec![0, 1, 0, 1, 0, 1], None).unwrap();
    let mesh = SceneMesh::with_distances(
        vec![
            two_triangles.with_material_slots(slots(&[1, 2])).unwrap(),
            level(1).with_material_slots(slots(&[3])).unwrap(),
        ],
        vec![1_000, 2_000],
    )
    .unwrap();
    // Materials 1 and 2 share a relief scale; 0 and 3 each have their own.
    let materials = [
        Material::default(),
        Material {
            impostor: [0.0, 0.5, 0.2],
            ..Material::default()
        },
        Material {
            wind: Vec3::new(2.0, 0.0, 0.0),
            impostor: [0.0, 0.5, 0.0],
            ..Material::default()
        },
        Material {
            curved: Some(Vec3::new(0.0, 0.3, 0.0)),
            impostor: [0.4, 1.25, 0.1],
            ..Material::default()
        },
    ];
    assert_matches_reference(&mesh, &materials);
}

fn cached(instances: &Instances) -> bool {
    let batch = instances.batches.values().next().unwrap();
    batch.bounds.borrow().is_cached(0)
}

fn selected(instances: &Instances, meshes: &[MeshSelection]) -> bool {
    let mut selection = Selection::default();
    selection.rebuild(instances, meshes, Mat4::IDENTITY, [0; 3]);
    selection.stats.culled == 0
}

fn caster_bits(instances: &Instances, meshes: &[MeshSelection]) -> [u32; 16] {
    caster_matrix(instances, meshes, None, Mat4::IDENTITY)
        .to_cols_array()
        .map(f32::to_bits)
}

#[test]
fn every_edit_refreshes_cached_bounds_for_selection_and_casters() {
    let meshes = [MeshSelection::new(&mesh())];
    let mut instances = Instances::new(&scene());
    let inside = transform([0, 0, 500]);
    let outside = transform([5_000, 0, 500]);
    // Upstream of the receiver along the light, so it extends the caster depth range.
    let upstream = transform([0, 0, -3_000]);
    let handle = instances.add(object(inside), true).unwrap();
    assert!(!cached(&instances));
    assert!(selected(&instances, &meshes));
    assert!(cached(&instances));

    instances.set_transform(handle, outside).unwrap();
    assert!(!cached(&instances));
    assert!(!selected(&instances, &meshes));

    instances.set_object(handle, object(inside)).unwrap();
    assert!(!cached(&instances));
    assert!(selected(&instances, &meshes));

    instances.set_visible(handle, false).unwrap();
    instances.set_visible(handle, true).unwrap();
    assert!(cached(&instances), "visibility does not move the placement");

    instances.remove(handle).unwrap();
    assert!(!cached(&instances));
    instances.add(object(upstream), true).unwrap();
    let mut fresh = Instances::new(&scene());
    fresh.add(object(upstream), true).unwrap();
    assert_eq!(
        caster_bits(&instances, &meshes),
        caster_bits(&fresh, &meshes)
    );
    assert_ne!(
        caster_bits(&instances, &meshes),
        Mat4::IDENTITY.to_cols_array().map(f32::to_bits),
        "the reused slot's new bounds extend the caster range"
    );
}
