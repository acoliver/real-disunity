//! A refresh with nothing left to compute is recognised from the batch's materials and mesh
//! alone, and every change to what a slot stores is still seen: the placements' edits, the
//! relief scale and fade reference of any material the batch draws with, and the mesh's levels.
use worldrunner_render_plan::SceneFadeDirection;

use super::super::large_scene::reference;
use super::*;
use crate::instances::fade::Fade;

fn slotted() -> SceneMesh {
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
    SceneMesh::with_distances(
        vec![
            two_triangles.with_material_slots(slots(&[1, 2])).unwrap(),
            level(1).with_material_slots(slots(&[3])).unwrap(),
        ],
        vec![1_000, 2_000],
    )
    .unwrap()
}

/// Materials 1 and 2 share a relief scale; 0 and 3 each have their own.
fn reliefs(scales: [f32; 4]) -> Vec<Material> {
    scales
        .map(|scale| Material {
            impostor: [0.0, scale, 0.0],
            ..Material::default()
        })
        .to_vec()
}

fn warm(selection: &MeshSelection, instances: &mut Instances) {
    instances.add(object(transform([0; 3])), true).unwrap();
    refresh(selection, instances);
}

fn refresh(selection: &MeshSelection, instances: &Instances) {
    let (&key, batch) = instances.batches.iter().next().unwrap();
    placement::refresh(selection, instances, key.2, batch);
}

fn current(selection: &MeshSelection, instances: &Instances) -> bool {
    let (&key, batch) = instances.batches.iter().next().unwrap();
    placement::is_current(selection, instances, key.2, batch)
}

fn stored_reliefs(instances: &Instances) -> Vec<u32> {
    let batch = instances.batches.values().next().unwrap();
    batch.bounds.borrow().reliefs().to_vec()
}

/// The distinct relief bits the batch's materials make, in material order.
fn expected_reliefs(selection: &MeshSelection, instances: &Instances) -> Vec<u32> {
    let (&key, _) = instances.batches.iter().next().unwrap();
    placement::BatchShape::new(selection, instances, key.2)
        .reliefs()
        .to_vec()
}

#[test]
fn unchanged_inputs_are_current_only_once_refreshed() {
    let selection = MeshSelection::new(&slotted());
    let mut instances = instances(&reliefs([0.0, 0.5, 0.5, 1.25]));
    let handle = instances.add(object(transform([0; 3])), true).unwrap();
    assert!(!current(&selection, &instances), "nothing is stored yet");

    refresh(&selection, &instances);
    assert!(current(&selection, &instances));
    refresh(&selection, &instances);
    assert!(current(&selection, &instances));

    instances
        .set_transform(handle, transform([4_000, 0, 0]))
        .unwrap();
    assert!(
        !current(&selection, &instances),
        "a moved slot has no boxes"
    );
    refresh(&selection, &instances);
    assert!(current(&selection, &instances));

    instances
        .add(object(transform([9_000, 0, 0])), true)
        .unwrap();
    assert!(!current(&selection, &instances), "a new slot has no boxes");
    refresh(&selection, &instances);
    assert!(current(&selection, &instances));
}

#[test]
fn a_changed_relief_scale_refills_the_boxes_to_match_the_reference() {
    let selection = MeshSelection::new(&slotted());
    let mut instances = instances(&reliefs([0.0, 0.5, 0.5, 1.25]));
    warm(&selection, &mut instances);

    instances.impostor_extents[3][1] = 2.0;
    assert!(!current(&selection, &instances));
    refresh(&selection, &instances);
    assert!(current(&selection, &instances));

    let placed = transform([0; 3]);
    let batch = instances.batches.values().next().unwrap();
    let cached = batch.bounds.borrow();
    assert_eq!(
        cached.reliefs(),
        [0.0_f32, 0.5, 2.0].map(f32::to_bits).as_slice()
    );
    for stretch in [0.0, 1.37] {
        assert_eq!(
            bits(placement::placement(
                cached.boxes(0),
                cached.reliefs(),
                stretch
            )),
            bits(selection.placement_bounds(&instances, &placed, 0, stretch)),
            "stretch {stretch}"
        );
    }
}

#[test]
fn relief_groups_are_compared_distinct_and_in_material_order() {
    let selection = MeshSelection::new(&slotted());
    let mut instances = instances(&reliefs([0.0, 0.5, 0.5, 1.25]));
    warm(&selection, &mut instances);

    // Another material takes the scale it already shares: the distinct scales are the same.
    instances.impostor_extents[2][1] = 0.5;
    assert!(current(&selection, &instances));

    // Same set of scales, other first appearance: the boxes are stored in another order.
    instances.impostor_extents[1][1] = 1.25;
    assert_eq!(
        expected_reliefs(&selection, &instances),
        [0.0_f32, 1.25, 0.5].map(f32::to_bits)
    );
    assert!(!current(&selection, &instances));
    refresh(&selection, &instances);
    assert_eq!(
        stored_reliefs(&instances),
        expected_reliefs(&selection, &instances)
    );

    // One group fewer: materials 0, 2 and 3 now share a scale, so only two remain.
    instances.impostor_extents[2][1] = 0.0;
    instances.impostor_extents[3][1] = 0.0;
    assert!(!current(&selection, &instances));
    refresh(&selection, &instances);
    assert_eq!(stored_reliefs(&instances).len(), 2);
    assert!(current(&selection, &instances));

    // A relief scale that differs only in its sign bit is another group.
    instances.impostor_extents[0][1] = -0.0;
    assert!(!current(&selection, &instances));
}

fn fading_from(instances: &mut Instances, material: usize, offset: [i32; 3]) {
    let scene = scene();
    let fading = scene.resources().materials()[0]
        .with_fade_direction(SceneFadeDirection::Keep)
        .with_fade_reference(reference(offset, None));
    instances.fades[material] = Fade::new(&fading, scene.resources().textures());
}

#[test]
fn a_changed_fade_reference_refills_the_inputs() {
    let selection = MeshSelection::new(&slotted());
    let mut instances = instances(&reliefs([0.0, 0.5, 0.5, 1.25]));
    fading_from(&mut instances, 3, [0, 0, 1_000]);
    warm(&selection, &mut instances);
    let references = |instances: &Instances| {
        let batch = instances.batches.values().next().unwrap();
        batch.bounds.borrow().inputs(0).references.to_vec()
    };
    assert_eq!(references(&instances).len(), 1);
    assert!(current(&selection, &instances));

    fading_from(&mut instances, 3, [0, 0, 2_000]);
    assert!(!current(&selection, &instances));
    refresh(&selection, &instances);
    assert!(current(&selection, &instances));
    assert_eq!(references(&instances), [reference([0, 0, 2_000], None)]);

    // Material 1 measures from the same point as material 3: still one distinct reference.
    fading_from(&mut instances, 1, [0, 0, 2_000]);
    assert!(current(&selection, &instances));
    assert_eq!(references(&instances).len(), 1);

    // Moving material 1's reference apart adds a second, in material order.
    fading_from(&mut instances, 1, [500, 0, 2_000]);
    assert!(!current(&selection, &instances));
    refresh(&selection, &instances);
    assert_eq!(
        references(&instances),
        [
            reference([500, 0, 2_000], None),
            reference([0, 0, 2_000], None)
        ]
    );
    assert!(current(&selection, &instances));
}

#[test]
fn a_mesh_with_another_level_count_is_not_current() {
    let selection = MeshSelection::new(&slotted());
    let mut instances = instances(&reliefs([0.0, 0.5, 0.5, 1.25]));
    warm(&selection, &mut instances);
    let shorter =
        MeshSelection::new(&SceneMesh::with_distances(vec![level(0)], vec![1_000]).unwrap());
    assert!(current(&selection, &instances));
    assert!(!current(&shorter, &instances));
}
