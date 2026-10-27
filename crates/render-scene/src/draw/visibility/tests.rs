#[test]
fn all_levels_and_camera_facing_rotations_have_conservative_bounds() {
    let near = level(0);
    let far = SceneMeshLod::new(
        1,
        vec![SceneVertex {
            tangent_micro: None,
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
                tangent_micro: None,
                position_millimeters: [-100, -200, -300],
                normal_micro: [0, 0, 1_000_000],
                texcoord_micro: [0; 2],
            },
            SceneVertex {
                tangent_micro: None,
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

#[test]
fn relieved_surface_fits_bounds_at_wide_frustum_corners() {
    // A 90-degree vertical, aspect-two camera has corner ray (2,1,-1).
    // A two-metre axial relief therefore moves by sqrt(24) metres.
    let matrix =
        glam::camera::rh::proj::directx::perspective(std::f32::consts::FRAC_PI_2, 2.0, 0.1, 100.0);
    let stretch = relief_ray_stretch(matrix);
    assert!((stretch - 6.0_f32.sqrt()).abs() < 0.0001);
    let placement = SceneTransform {
        position_millimeters: [0; 3],
        rotation: worldrunner_render_plan::SceneRotation::IDENTITY,
        scale_millionths: 1_000_000,
    };
    let padding = impostor_extent([0.0, 2.0], &placement, stretch);
    let bounds = Bounds {
        min: Vec3::splat(-1.0),
        max: Vec3::splat(1.0),
    }
    .expanded(padding);
    for sign in [-1.0, 1.0] {
        let surface = Vec3::splat(sign) + Vec3::new(4.0, 2.0, 2.0);
        assert!(surface.cmpge(bounds.min).all() && surface.cmple(bounds.max).all());
    }
    assert_eq!(impostor_extent([0.0; 2], &placement, stretch), Vec3::ZERO);
}

#[test]
fn relief_survives_culling_when_only_displaced_surface_intersects() {
    let mut instances = Instances::new(&scene());
    instances
        .add(
            SceneObject::new(
                transform([0, 0, 3000]),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            ),
            true,
        )
        .unwrap();
    let meshes = [MeshSelection::new(&mesh())];
    let mut selection = Selection::default();
    selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0; 3]);
    assert_eq!(selection.stats.culled, 1);
    instances.impostor_extents[0] = [0.0, 2.0, 0.0];
    selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0; 3]);
    // The card spans Z=2.7..3.3; its two-metre relief spans Z=0.7..1.3.
    assert_eq!(selection.stats.culled, 0);
}

#[test]
fn enlarged_plane_with_oblique_curvature_keeps_analytically_visible_tip() {
    // Integrate a tangent cos(k*s)*Z + sin(k*s)*normalize(X+Z),
    // for a 40-metre capture plane's upper tip, with k=0.05 and s=20.
    let force = Vec3::new(1.0, 0.0, 1.0).normalize();
    let tip = Vec3::Z * (1.0_f32.sin() / 0.05) + force * ((1.0 - 1.0_f32.cos()) / 0.05);
    assert!(tip.z > 23.0);
    let placement = transform([-6501, 0, -22830]);
    let point = millimeters_to_meters(placement.position_millimeters) + tip;
    assert!(Frustum::new(Mat4::IDENTITY).intersects(Bounds {
        min: point,
        max: point
    }));
    let source = enlarged_wind_scene();
    let mut instances = Instances::new(&source);
    assert!((instances.impostor_extents[0][0] - 20.0).abs() < 0.0001);
    assert_eq!(instances.curved_wind[0], Some(Vec3::ZERO));
    instances
        .add(
            SceneObject::new(
                placement,
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            ),
            true,
        )
        .unwrap();
    let meshes = [MeshSelection::new(&source.geometry().meshes()[0])];
    let late_shape = meshes[0]
        .wind_bounds(&placement, Vec3::ZERO, Some(Vec3::ZERO), [0.0; 2])
        .expanded(impostor_extent([20.0, 0.0], &placement, 1.0));
    assert!(!Frustum::new(Mat4::IDENTITY).intersects(late_shape));
    let mut selection = Selection::default();
    selection.rebuild(&instances, &meshes, Mat4::IDENTITY, [0; 3]);
    assert_eq!(selection.stats.culled, 0);
    let bounds = meshes[0].material_bounds(
        &placement,
        Vec3::ZERO,
        Some(Vec3::ZERO),
        [20.0, 0.0, 0.0],
        1.0,
    );
    assert!(point.cmpge(bounds.min).all() && point.cmple(bounds.max).all());
}

#[test]
fn enlarged_card_overlapping_receiver_xy_extends_caster_depth() {
    let mut instances = Instances::new(&scene());
    instances.impostor_extents[0] = [4.0, 0.0, 0.0];
    instances
        .add(
            SceneObject::new(
                transform([3000, 0, -5000]),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            ),
            true,
        )
        .unwrap();
    let meshes = [MeshSelection::new(&mesh())];
    let matrix = caster_matrix(&instances, &meshes, None, Mat4::IDENTITY);
    // The source bounds miss receiver X, but the enlarged card reaches X=0.
    let upstream = matrix.project_point3(Vec3::new(0.0, 0.0, -8.0));
    assert!((0.0..=1.0).contains(&upstream.z));
}

fn enlarged_wind_scene() -> RenderScene {
    let base = scene();
    let band = SceneWindBand::new(1_000_000, 0, 0).unwrap();
    let foliage = SceneFoliageWind::new(
        SceneDirection::new([0, 0, 1_000_000]).unwrap(),
        [20_000, 1],
        1_000_000,
        [band; 2],
        SceneWindPhase::WorldPosition,
    )
    .unwrap();
    // For extent=20 and force=20*tan(0.5), curvature is 0.05.
    let wind = SceneWind::new(
        SceneDirection::new([707_107, 0, 707_107]).unwrap(),
        10_926,
        1000,
        0,
    )
    .unwrap()
    .with_foliage(foliage);
    let atlas = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
        .unwrap()
        .with_plane_size([1, 40_000])
        .unwrap()
        .with_borders(TextureHandle::from_raw(1));
    let material = base.resources().materials()[0]
        .with_impostor(atlas)
        .with_features(SceneMaterialFeatures {
            tint: SceneTint::WHITE,
            ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
            wind: Some(wind),
            layer: None,
            translucency: None,
        })
        .unwrap();
    let mut textures = base.resources().textures().to_vec();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0; 4].into(),
    });
    let mesh = small_capture_card();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![material]).unwrap(),
        geometry: SceneGeometry::new(vec![mesh], vec![]).unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

fn small_capture_card() -> SceneMesh {
    let vertices = [
        ([-1, 0, -1], [0, 0]),
        ([1, 0, -1], [1_000_000, 0]),
        ([1, 0, 1], [1_000_000, 1_000_000]),
        ([-1, 0, 1], [0, 1_000_000]),
    ]
    .into_iter()
    .map(|(position, uv)| SceneVertex {
        position_millimeters: position,
        normal_micro: [0, -1_000_000, 0],
        texcoord_micro: uv,
        tangent_micro: None,
    })
    .collect();
    SceneMesh::new(vec![
        SceneMeshLod::new(
            0,
            vertices,
            vec![0, 1, 2, 0, 2, 3],
            Some(BillboardPolicy::YawLocked),
        )
        .unwrap(),
    ])
    .unwrap()
}

#[test]
fn slot_material_displacement_expands_culling_and_caster_bounds() {
    let lod = level(0)
        .with_material_slots(vec![
            SceneLodMaterialSlot::new(MaterialHandle::from_raw(1), 0..3).unwrap(),
        ])
        .unwrap();
    let selection = MeshSelection::new(&SceneMesh::new(vec![lod]).unwrap());
    let mut instances = Instances::new(&scene());
    instances.wind_extents.push(Vec3::new(2.0, 0.0, 0.0));
    instances.curved_wind.push(None);
    instances.impostor_extents.push([0.0; 3]);
    let placement = transform([0; 3]);
    let original = selection.world_bounds(&placement);
    let expanded = selection.placement_bounds(&instances, &placement, 0, 1.0);
    assert!(expanded.min.x < -2.1 && expanded.max.x > 2.1);
    assert_eq!(expanded.min.y.to_bits(), original.min.y.to_bits());
    assert_eq!(expanded.max.z.to_bits(), original.max.z.to_bits());
}
