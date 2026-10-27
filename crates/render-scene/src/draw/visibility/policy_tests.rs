use super::*;
use worldrunner_render_plan::*;

fn placement() -> SceneTransform {
    SceneTransform {
        position_millimeters: [0; 3],
        rotation: SceneRotation::IDENTITY,
        scale_millionths: 1_000_000,
    }
}
fn mesh() -> SceneMesh {
    let vertices = [[-100, -100, 0], [100, 100, 200], [0; 3]]
        .map(|position_millimeters| SceneVertex {
            tangent_micro: None,
            position_millimeters,
            normal_micro: [0, 0, 1_000_000],
            texcoord_micro: [0; 2],
        })
        .to_vec();
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2], Some(BillboardPolicy::Spherical)).unwrap(),
    ])
    .unwrap()
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
        double_sided: true,
    })
    .unwrap();
    RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![SceneTexture::Texels {
                format: TexelFormat::Rgba8Unorm,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![255; 4].into(),
            }],
            vec![material],
        )
        .unwrap(),
        geometry: SceneGeometry::new(
            vec![mesh()],
            vec![SceneObject::new(
                placement(),
                MeshHandle::from_raw(0),
                MaterialHandle::from_raw(0),
            )],
        )
        .unwrap(),
        terrain: None,
        effects: None,
    })
    .unwrap()
}

#[test]
fn capture_center_and_curved_wind_enclose_tip_for_selection_and_caster_fitting() {
    // Integrate the parallel, additive tangent basis at a 100m capture offset.
    let curvature = 2.0 * (54.630_f64 / 100.0).atan() / 100.0;
    let angle = 100.0 * curvature;
    let tip_z = -(angle.sin() + 1.0 - angle.cos()) / curvature;
    assert!((tip_z + 130.11684).abs() < 0.001);
    let point = Vec3::new(0.0, 0.0, -130.11684);
    let meshes = [MeshSelection::new(&mesh())];
    let mut instances = Instances::new(&scene());
    instances.curved_wind[0] = Some(Vec3::ZERO);
    instances.impostor_extents[0] = [0.0, 0.0, 100.0];
    let bounds = meshes[0].placement_bounds(&instances, &placement(), 0, 1.0);
    assert!(point.cmpge(bounds.min).all() && point.cmple(bounds.max).all());
    let mut camera = Mat4::IDENTITY;
    camera.w_axis.z = 130.5;
    assert!((0.0..=1.0).contains(&camera.project_point3(point).z));
    let mut selected = Selection::default();
    selected.rebuild(&instances, &meshes, camera, [0; 3]);
    assert_eq!(selected.stats.culled, 0);
    let fitted = caster_matrix(&instances, &meshes, None, Mat4::IDENTITY);
    assert!((0.0..=1.0).contains(&fitted.project_point3(point).z));
    selected.rebuild(&instances, &meshes, fitted, [0; 3]);
    assert_eq!(selected.stats.culled, 0);
    assert!((fitted.project_point3(Vec3::Z).z - 1.0).abs() < 1e-6);
}

#[test]
fn capture_center_padding_and_zero_offset_preserve_rigid_and_curved_bounds() {
    let selected = MeshSelection::new(&mesh());
    let base = selected.world_bounds(&placement());
    let rigid = selected.material_bounds(&placement(), Vec3::ZERO, None, [0.0, 0.0, 10.0], 1.0);
    assert_eq!(rigid.min, base.min - Vec3::splat(10.0));
    assert_eq!(rigid.max, base.max + Vec3::splat(10.0));
    let radius = selected
        .bounds
        .min
        .abs()
        .max(selected.bounds.max.abs())
        .length()
        .max(selected.billboard_radius.unwrap());
    let origin = Vec3::new(0.0, 0.0, 0.3);
    let original = base.expanded(Vec3::splat(6.0 * radius + 7.0 * origin.length()));
    let neutral = selected.material_bounds(&placement(), Vec3::ZERO, Some(origin), [0.0; 3], 1.0);
    assert_eq!(original.min, neutral.min);
    assert_eq!(original.max, neutral.max);
}
