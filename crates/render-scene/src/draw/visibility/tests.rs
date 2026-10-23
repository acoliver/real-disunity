#[test]
fn all_levels_and_camera_facing_rotations_have_conservative_bounds() {
    let near = level(0);
    let far = SceneMeshLod::new(
        1,
        vec![SceneVertex {
            position_millimeters: [2_000, 0, 0],
            normal_micro: [0, 0, 1_000_000],
            texcoord_micro: [0; 2],
        }],
        vec![0, 0, 0],
        Some(BillboardPolicy::Spherical),
    )
    .unwrap();
    let mesh = SceneMesh::with_distances(vec![near, far], vec![1_000, 2_000]).unwrap();
    let selection = MeshSelection::new(&mesh);
    let mut placement = transform([0, 0, 500]);
    placement.scale_millionths = 2_000_000;
    let bounds = selection.world_bounds(&placement);
    for direction in [Vec3::X, -Vec3::X, Vec3::Y, -Vec3::Y, Vec3::Z, -Vec3::Z] {
        let point = Vec3::new(0.0, 0.0, 0.5) + direction * 4.0;
        assert!(point.cmpge(bounds.min).all() && point.cmple(bounds.max).all());
    }
}

#[test]
fn light_and_camera_views_share_lod_but_not_visibility() {
    let scene = scene();
    let mut instances = Instances::new(&scene);
    instances
        .add(
            SceneObject::new(
                transform([0, 0, -3_000]),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            ),
            true,
        )
        .unwrap();
    let meshes = [MeshSelection::new(&mesh())];
    let mut camera = Selection::default();
    let mut shadow = Selection::default();
    let eye = [0, 0, -2_000];
    camera.rebuild(&instances, &meshes, Mat4::IDENTITY, eye);
    shadow.rebuild(
        &instances,
        &meshes,
        caster_matrix(&instances, &meshes, None, Mat4::IDENTITY),
        eye,
    );
    assert_eq!(camera.stats.culled, 1);
    assert_eq!(shadow.stats.drawn_per_lod, vec![1, 0, 0]);
    shadow.rebuild(
        &instances,
        &meshes,
        caster_matrix(&instances, &meshes, None, Mat4::IDENTITY),
        [0; 3],
    );
    assert_eq!(shadow.stats.drawn_per_lod, vec![0, 0, 1]);
}

use super::*;
use worldrunner_render_plan::*;

fn level(index: u8) -> SceneMeshLod {
    SceneMeshLod::new(
        index,
        vec![
            SceneVertex {
                position_millimeters: [-100, -200, -300],
                normal_micro: [0, 0, 1_000_000],
                texcoord_micro: [0; 2],
            },
            SceneVertex {
                position_millimeters: [100, 200, 300],
                normal_micro: [0, 0, 1_000_000],
                texcoord_micro: [0; 2],
            },
        ],
        vec![0, 1, 0],
        None,
    )
    .unwrap()
}

fn mesh() -> SceneMesh {
    SceneMesh::with_distances(
        vec![level(0), level(1), level(2)],
        vec![1_000, 2_000, 4_000],
    )
    .unwrap()
}

fn transform(position: [i64; 3]) -> SceneTransform {
    SceneTransform {
        position_millimeters: position,
        rotation: SceneRotation::IDENTITY,
        scale_millionths: 1_000_000,
    }
}

#[test]
fn thresholds_are_inclusive_and_the_last_level_extends_to_infinity() {
    let selection = MeshSelection::new(&mesh());
    for (distance, expected) in [
        (0_u128, 0),
        (999, 0),
        (1_000, 0),
        (1_001, 1),
        (2_000, 1),
        (2_001, 2),
        (4_000, 2),
        (100_000, 2),
    ] {
        assert_eq!(selection.lod(distance * distance), expected);
    }
}

#[test]
fn rebuild_preserves_authored_boundaries_for_camera_and_shadow() {
    for (eye_x, ceiling) in [(0, 100_u64), (0, 1_000), (100, 1_000)] {
        let eye = [eye_x, 0, 500];
        let view = crate::view::SceneView::new(
            crate::view::CameraPose {
                eye_millimeters: eye,
                target_millimeters: [eye_x + 1_000, 0, 500],
            },
            SceneCamera {
                vertical_fov_millidegrees: 60_000,
                near_plane_millimeters: 1,
                far_plane_millimeters: 10_000,
            },
            crate::view::Viewport {
                width: 64,
                height: 64,
            },
        )
        .unwrap();
        let frame = crate::frame::FrameUniform::new(&view, None).unwrap();
        assert_eq!(frame.camera_position_millimeters, eye);
        let meshes = [MeshSelection::new(
            &SceneMesh::with_distances(
                vec![level(0), level(1), level(2)],
                vec![ceiling, ceiling * 2, ceiling * 4],
            )
            .unwrap(),
        )];
        for (outside, expected) in [(0, 0), (1, 1)] {
            let mut instances = Instances::new(&scene());
            instances
                .add(
                    SceneObject::new(
                        transform([eye_x + i64::try_from(ceiling).unwrap() + outside, 0, 500]),
                        MeshHandle::from_raw(0),
                        MaterialHandle::from_raw(0),
                    ),
                    true,
                )
                .unwrap();
            let receiver = Mat4::from_scale(Vec3::splat(0.1));
            for matrix in [
                frame.view_projection,
                caster_matrix(&instances, &meshes, None, receiver),
            ] {
                let mut selection = Selection::default();
                selection.rebuild(
                    &instances,
                    &meshes,
                    matrix,
                    frame.camera_position_millimeters,
                );
                assert_eq!(selection.stats.considered, 1);
                assert_eq!(selection.stats.culled, 0);
                assert_eq!(selection.ranges.len(), 1);
                assert_eq!(selection.ranges[0].lod, expected);
                assert_eq!(selection.stats.drawn_per_lod[expected], 1);
            }
        }
    }
}

#[test]
fn missing_zero_duplicate_descending_and_wrong_count_thresholds_fail() {
    for ceilings in [
        vec![],
        vec![0, 2, 3],
        vec![1, 1, 3],
        vec![2, 1, 3],
        vec![1, 2],
        vec![1, 2, 3, 4],
    ] {
        assert_eq!(
            SceneMesh::with_distances(vec![level(0), level(1), level(2)], ceilings).unwrap_err(),
            SceneValidationError::LodDistances
        );
    }
    assert_eq!(
        SceneMesh::new(vec![level(0), level(1)]).unwrap_err(),
        SceneValidationError::LodDistances
    );
    assert!(SceneMesh::new(vec![level(0)]).is_ok());
}

#[test]
fn clip_volume_keeps_inside_intersecting_and_boundary_boxes() {
    let frustum = Frustum::new(Mat4::IDENTITY);
    for bounds in [
        Bounds {
            min: Vec3::new(-0.2, -0.2, 0.2),
            max: Vec3::new(0.2, 0.2, 0.8),
        },
        Bounds {
            min: Vec3::new(0.9, -0.2, 0.2),
            max: Vec3::new(1.2, 0.2, 0.8),
        },
        Bounds {
            min: Vec3::new(-1.0, -1.0, 0.0),
            max: Vec3::new(1.0, 1.0, 1.0),
        },
    ] {
        assert!(frustum.intersects(bounds));
    }
    for center in [
        Vec3::new(2.0, 0.0, 0.5),
        Vec3::new(-2.0, 0.0, 0.5),
        Vec3::new(0.0, 2.0, 0.5),
        Vec3::new(0.0, -2.0, 0.5),
        Vec3::new(0.0, 0.0, -1.0),
        Vec3::new(0.0, 0.0, 2.0),
    ] {
        assert!(!frustum.intersects(Bounds {
            min: center - Vec3::splat(0.1),
            max: center + Vec3::splat(0.1)
        }));
    }
}

#[test]
fn rotated_scaled_bounds_enclose_every_transformed_corner() {
    let mesh = MeshSelection::new(&mesh());
    let mut placement = transform([1_300, 0, 500]);
    assert!(!Frustum::new(Mat4::IDENTITY).intersects(mesh.world_bounds(&placement)));
    placement.rotation = SceneRotation {
        x: 0,
        y: 0,
        z: 707_107,
        w: 707_107,
    };
    placement.scale_millionths = 2_000_000;
    let bounds = mesh.world_bounds(&placement);
    assert!(Frustum::new(Mat4::IDENTITY).intersects(bounds));
    let model = model_matrix(&placement);
    for x in [-0.1, 0.1] {
        for y in [-0.2, 0.2] {
            for z in [-0.3, 0.3] {
                let p = model.transform_point3(Vec3::new(x, y, z));
                assert!(p.cmpge(bounds.min).all() && p.cmple(bounds.max).all());
            }
        }
    }
}

fn scene() -> RenderScene {
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space: TextureColorSpace::Srgb,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![255; 4].into(),
            }],
            vec![material],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![mesh()], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

#[test]
fn handle_edits_rebuild_view_membership_without_stale_holes_and_reuse_ranges() {
    let scene = scene();
    let mut instances = Instances::new(&scene);
    let meshes = [MeshSelection::new(&mesh())];
    let object = |p| {
        SceneObject::new(
            transform(p),
            MeshHandle::from_raw(0),
            MaterialHandle::from_raw(0),
        )
    };
    let first = instances.add(object([0, 0, 500]), true).unwrap();
    let middle = instances.add(object([0, 0, 500]), true).unwrap();
    let last = instances.add(object([0, 0, 500]), true).unwrap();
    let mut selection = Selection::default();
    selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0; 3]);
    assert_eq!(selection.ranges[0].slots, 0..3);
    instances.set_visible(middle, false).unwrap();
    instances
        .set_transform(last, transform([3_000, 0, 500]))
        .unwrap();
    selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0; 3]);
    assert_eq!(
        selection.stats,
        ViewStats {
            considered: 3,
            culled: 2,
            drawn_per_lod: vec![1, 0, 0]
        }
    );
    assert_eq!(selection.ranges[0].slots, 0..1);
    instances.remove(first).unwrap();
    let replacement = instances.add(object([0, 0, 500]), true).unwrap();
    assert_ne!(first, replacement);
    assert!(instances.set_visible(first, true).is_err());
    instances.set_visible(middle, true).unwrap();
    instances
        .set_transform(last, transform([0, 0, 500]))
        .unwrap();
    selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0, 0, -2_000]);
    let capacity = selection.ranges.capacity();
    assert_eq!(selection.ranges[0].slots, 0..3);
    assert_eq!(selection.stats.drawn_per_lod, vec![0, 0, 3]);
    selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0; 3]);
    assert_eq!(selection.ranges.capacity(), capacity);
}

#[test]
fn shadow_extrusion_retains_upstream_casters_outside_the_camera_and_receiver_box() {
    let scene = scene();
    let mut instances = Instances::new(&scene);
    let object = SceneObject::new(
        transform([0, 0, -3_000]),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let handle = instances.add(object, true).unwrap();
    let meshes = [MeshSelection::new(&mesh())];
    let bounds = meshes[0].world_bounds(object.transform());
    assert!(!Frustum::new(Mat4::IDENTITY).intersects(bounds));
    let matrix = caster_matrix(&instances, &meshes, None, Mat4::IDENTITY);
    assert!(Frustum::new(matrix).intersects(bounds));
    assert!((matrix.project_point3(Vec3::Z).z - 1.0).abs() < 1e-6);
    instances.set_visible(handle, false).unwrap();
    assert_eq!(
        caster_matrix(&instances, &meshes, None, Mat4::IDENTITY),
        Mat4::IDENTITY
    );
}

#[test]
fn terrain_bounds_extend_only_overlapping_light_depth_and_preserve_far_plane() {
    let instances = Instances::new(&scene());
    let meshes = [MeshSelection::new(&mesh())];
    let terrain = Bounds {
        min: Vec3::new(-0.5, -0.5, -4.0),
        max: Vec3::new(0.5, 0.5, -2.0),
    };
    let matrix = caster_matrix(&instances, &meshes, Some(terrain), Mat4::IDENTITY);
    assert!(Frustum::new(matrix).intersects(terrain));
    assert!(matrix.project_point3(terrain.min).z.abs() < 1e-6);
    assert!((matrix.project_point3(Vec3::Z).z - 1.0).abs() < 1e-6);
    let outside = Bounds {
        min: terrain.min + Vec3::X * 3.0,
        max: terrain.max + Vec3::X * 3.0,
    };
    assert_eq!(
        caster_matrix(&instances, &meshes, Some(outside), Mat4::IDENTITY),
        Mat4::IDENTITY
    );
}
