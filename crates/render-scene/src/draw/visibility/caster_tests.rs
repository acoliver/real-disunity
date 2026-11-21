use super::tests::{mesh, scene, transform};
use super::*;
use worldrunner_render_plan::{MaterialHandle, MeshHandle, SceneObject};

#[test]
fn non_caster_does_not_extend_fit_but_remains_in_camera_selection() {
    let source = scene();
    let mut instances = Instances::new(&source);
    let meshes = [MeshSelection::new(&mesh())];
    let object = SceneObject::new(
        transform([0, 0, -3_000]),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    assert!(object.casts_shadow());
    let handle = instances
        .add(object.with_casts_shadow(false), true)
        .unwrap();
    assert_eq!(
        caster_matrix(&instances, &meshes, None, Mat4::IDENTITY),
        Mat4::IDENTITY
    );
    instances.set_object(handle, object).unwrap();
    let extended = caster_matrix(&instances, &meshes, None, Mat4::IDENTITY);
    assert_ne!(extended, Mat4::IDENTITY);
    instances
        .set_object(handle, object.with_casts_shadow(false))
        .unwrap();
    let mut camera = Selection::default();
    camera.rebuild(&instances, &meshes, extended, [0; 3]);
    let mut casters = Selection {
        casters_only: true,
        ..Selection::default()
    };
    casters.rebuild(&instances, &meshes, extended, [0; 3]);
    assert_eq!(camera.stats.drawn_per_lod.iter().sum::<usize>(), 1);
    assert_eq!(casters.stats.drawn_per_lod.iter().sum::<usize>(), 0);
}

#[test]
fn shadow_only_placement_casts_and_extends_fit_but_leaves_camera_selection() {
    let source = scene();
    let mut instances = Instances::new(&source);
    let meshes = [MeshSelection::new(&mesh())];
    let object = SceneObject::new(
        transform([0, 0, -3_000]),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    assert!(!object.shadow_only());
    let handle = instances.add(object.with_shadow_only(true), true).unwrap();
    let extended = caster_matrix(&instances, &meshes, None, Mat4::IDENTITY);
    assert_ne!(extended, Mat4::IDENTITY);
    let counts = |instances: &Instances| {
        let mut camera = Selection::default();
        camera.rebuild(instances, &meshes, extended, [0; 3]);
        let mut casters = Selection {
            casters_only: true,
            ..Selection::default()
        };
        casters.rebuild(instances, &meshes, extended, [0; 3]);
        [camera, casters].map(|selection| selection.stats.drawn_per_lod.iter().sum::<usize>())
    };
    assert_eq!(counts(&instances), [0, 1]);
    // Shadow-only adds nothing when the placement does not cast.
    instances
        .set_object(
            handle,
            object.with_shadow_only(true).with_casts_shadow(false),
        )
        .unwrap();
    assert_eq!(counts(&instances), [0, 0]);
    instances.set_object(handle, object).unwrap();
    assert_eq!(counts(&instances), [1, 1]);
    // A hidden placement stays out of the casters too.
    instances.set_visible(handle, false).unwrap();
    instances
        .set_object(handle, object.with_shadow_only(true))
        .unwrap();
    assert_eq!(counts(&instances), [0, 0]);
}
