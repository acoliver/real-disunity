use glam::{Vec2, Vec3};
use worldrunner_camera::{
    CameraLens, CameraPose, CameraState, ScreenRay, ViewConvention, ViewHandedness, Viewport,
};
use worldrunner_render_plan::*;

fn material(double_sided: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided,
    })
    .unwrap()
}

fn scene() -> RenderScene {
    let vertices = [[-1000, -1000, 0], [1000, -1000, 0], [0, 1000, 0]]
        .map(|position_millimeters| SceneVertex {
            tangent_micro: None,
            position_millimeters,
            normal_micro: [0, 0, 1_000_000],
            texcoord_micro: [0; 2],
        })
        .to_vec();
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2], None).unwrap(),
    ])
    .unwrap();
    let material = material(false);
    let resources = SceneResources::new(
        vec![SceneTexture::Texels {
            format: TexelFormat::Rgba8Unorm,
            color_space: TextureColorSpace::Linear,
            width: 1,
            height: 1,
            mips: 1,
            bytes: vec![255; 4].into_boxed_slice(),
        }],
        vec![material],
    )
    .unwrap();
    RenderScene::new(SceneParts {
        resources,
        geometry: SceneGeometry::new(vec![mesh], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn object(z: i64) -> SceneObject {
    SceneObject::new(
        SceneTransform {
            position_millimeters: [0, 0, z],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
}
fn ray() -> ScreenRay {
    ScreenRay {
        origin_meters: Vec3::new(0.0, 0.0, 10.0),
        direction: Vec3::NEG_Z,
    }
}

#[test]
fn nearest_hit_ties_filters_and_misses() {
    let scene = scene();
    let objects = [
        (ObjectId(8), object(0)),
        (ObjectId(4), object(2000)),
        (ObjectId(2), object(2000)),
    ];
    let hit = scene.pick(ray(), objects, |_| true).unwrap();
    assert_eq!(hit.object, ObjectId(2));
    assert!((hit.distance_meters - 8.0).abs() < 1e-6);
    assert!(hit.point_meters.abs_diff_eq(Vec3::new(0.0, 0.0, 2.0), 1e-6));
    assert_eq!(
        scene
            .pick(ray(), objects, |id| id == ObjectId(8))
            .unwrap()
            .object,
        ObjectId(8)
    );
    assert!(scene.pick(ray(), objects, |_| false).is_none());
    assert!(
        scene
            .pick(
                ScreenRay {
                    direction: Vec3::X,
                    ..ray()
                },
                objects,
                |_| true
            )
            .is_none()
    );
    assert!(
        scene
            .pick(
                ScreenRay {
                    direction: Vec3::Z,
                    ..ray()
                },
                objects,
                |_| true
            )
            .is_none()
    );
    let reversed = objects.into_iter().rev();
    assert_eq!(scene.pick(ray(), reversed, |_| true), Some(hit));
}

#[test]
fn translated_rotated_scaled_and_nonuniform_geometry() {
    let scene = scene();
    let transform = SceneTransform {
        position_millimeters: [3000, 2000, 1000],
        rotation: SceneRotation {
            x: 0,
            y: 707_107,
            z: 0,
            w: 707_107,
        },
        scale_millionths: 2_000_000,
    };
    let ray = ScreenRay {
        origin_meters: Vec3::new(10.0, 2.0, 1.0),
        direction: Vec3::NEG_X,
    };
    let hit = scene
        .pick(
            ray,
            [(ObjectId(1), object(0).with_transform(transform))],
            |_| true,
        )
        .unwrap();
    assert!(hit.point_meters.abs_diff_eq(Vec3::new(3.0, 2.0, 1.0), 1e-5));
    assert!((hit.distance_meters - 7.0).abs() < 1e-5);
    // SceneTransform has uniform scale only. Non-uniform authoring is resident vertex geometry.
    let mut lod = scene.geometry().meshes()[0].lods()[0].vertices().to_vec();
    for vertex in &mut lod {
        vertex.position_millimeters[0] *= 4;
    }
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, lod, vec![0, 1, 2], None).unwrap(),
    ])
    .unwrap();
    let scene = scene
        .with_resources(
            scene.resources().clone(),
            SceneGeometry::new(vec![mesh], vec![]).unwrap(),
            None,
        )
        .unwrap();
    let ray = ScreenRay {
        origin_meters: Vec3::new(2.0, -0.5, 10.0),
        ..self::ray()
    };
    assert!(
        scene
            .pick(ray, [(ObjectId(0), object(0))], |_| true)
            .is_some()
    );
}

#[test]
fn back_faces_respect_material_sidedness() {
    let scene = scene();
    let ray = ScreenRay {
        origin_meters: Vec3::NEG_Z,
        direction: Vec3::Z,
    };
    assert!(
        scene
            .pick(ray, [(ObjectId(0), object(0))], |_| true)
            .is_none()
    );
    let material = material(true);
    let resources = scene
        .resources()
        .clone()
        .with_materials(vec![material])
        .unwrap();
    let scene = scene
        .with_resources(resources, scene.geometry().clone(), None)
        .unwrap();
    assert!(
        scene
            .pick(ray, [(ObjectId(0), object(0))], |_| true)
            .is_some()
    );
}

#[test]
fn tall_object_top_precedes_ground_in_both_camera_conventions() {
    let scene = scene();
    for handedness in [
        ViewHandedness::RightHanded,
        ViewHandedness::DirectXLeftHanded,
    ] {
        let view = worldrunner_camera::resolve(&CameraState {
            pose: CameraPose {
                eye_meters: glam::DVec3::new(0.0, -10.0, 10.0),
                forward: glam::DVec3::new(0.0, 10.0, -5.0),
                up: glam::DVec3::Z,
            },
            lens: CameraLens::Perspective {
                vertical_fov_radians: 1.0,
                near_meters: 0.1,
                far_meters: 100.0,
            },
            viewport: Viewport {
                width: 100,
                height: 100,
            },
            convention: ViewConvention { handedness },
        })
        .unwrap();
        let ray = view.screen_ray(Vec2::splat(50.0)).unwrap();
        let ground = ray.intersect_plane(Vec3::ZERO, Vec3::Z).unwrap();
        assert!(ground.y > 9.0);
        let hit = scene
            .pick(ray, [(ObjectId(0), object(5000))], |_| true)
            .unwrap();
        assert!(hit.point_meters.abs_diff_eq(Vec3::new(0.0, 0.0, 5.0), 1e-4));
        let point = Vec3::new(0.25, 0.0, 5.0);
        let screen = view.world_to_screen(point).unwrap();
        assert_eq!(screen.x > 50.0, handedness == ViewHandedness::RightHanded);
        let ray = view.screen_ray(screen.truncate()).unwrap();
        let hit = scene
            .pick(ray, [(ObjectId(0), object(5000))], |_| true)
            .unwrap();
        assert!(hit.point_meters.abs_diff_eq(point, 1e-4));
    }
}

#[test]
fn lod_material_slots_override_placement_sidedness() {
    let scene = scene();
    let vertices = scene.geometry().meshes()[0].lods()[0].vertices().to_vec();
    let slots = vec![
        SceneLodMaterialSlot::new(MaterialHandle::from_raw(0), 0..3).unwrap(),
        SceneLodMaterialSlot::new(MaterialHandle::from_raw(1), 3..6).unwrap(),
    ];
    let lod = SceneMeshLod::new(0, vertices, [0, 1, 2].repeat(2), None)
        .unwrap()
        .with_material_slots(slots)
        .unwrap();
    let geometry = SceneGeometry::new(vec![SceneMesh::new(vec![lod]).unwrap()], vec![]).unwrap();
    let resources = scene
        .resources()
        .clone()
        .with_materials(vec![material(false), material(true)])
        .unwrap();
    let scene = scene.with_resources(resources, geometry, None).unwrap();
    let ray = ScreenRay {
        origin_meters: Vec3::NEG_Z,
        direction: Vec3::Z,
    };
    let hit = scene
        .pick(ray, [(ObjectId(0), object(0))], |_| true)
        .unwrap();
    assert_eq!(hit.triangle, 1);
    assert!((hit.barycentric.iter().sum::<f64>() - 1.0).abs() < 1e-12);
}

#[test]
fn measure_pick_cost() {
    let base = scene();
    let vertices = base.geometry().meshes()[0].lods()[0].vertices().to_vec();
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, [0, 1, 2].repeat(100), None).unwrap(),
    ])
    .unwrap();
    let scene = base
        .with_resources(
            base.resources().clone(),
            SceneGeometry::new(vec![mesh], vec![]).unwrap(),
            None,
        )
        .unwrap();
    let objects: Vec<_> = (0..1000).map(|id| (ObjectId(id), object(0))).collect();
    for (label, ray) in [
        ("all bounds hit", ray()),
        (
            "all bounds rejected",
            ScreenRay {
                origin_meters: Vec3::new(10.0, 0.0, 10.0),
                ..ray()
            },
        ),
    ] {
        let start = std::time::Instant::now();
        for _ in 0..10 {
            std::hint::black_box(scene.pick(ray, objects.iter().copied(), |_| true));
        }
        eprintln!(
            "pick {label}: 1000 objects x 100 triangles, 10 queries: {:?}",
            start.elapsed()
        );
    }
}
