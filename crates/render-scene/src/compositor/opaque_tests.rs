use super::*;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneAlphaMode, SceneGeometry, SceneMaterial, SceneMaterialParts,
    SceneMesh, SceneMeshLod, SceneObject, SceneParts, SceneResources, SceneRotation, SceneTerrain,
    SceneTerrainAppearance, SceneTerrainParts, SceneTerrainSurface, SceneTexture, SceneTransform,
    SceneVertex, TerrainLodPolicy, TexelFormat, TextureColorSpace, TextureHandle,
};

fn prepared() -> PreparedScene {
    let texture = |value| SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![value, value, value, 255].into_boxed_slice(),
    };
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let terrain = SceneTerrain::new(SceneTerrainParts {
        columns: 2,
        rows: 2,
        origin_east_millimeters: 0,
        origin_north_millimeters: 0,
        sample_spacing_millimeters: 100,
        heights_millimeters: vec![0, 10, 5, 15],
        surface_indices: vec![0; 4],
        surfaces: vec![SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance::NEUTRAL,
        )],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap();
    let vertices = vec![
        SceneVertex {
            position_millimeters: [0, 0, 0],
            normal_micro: [0, 0, 1_000_000],
            tangent_micro: None,
            texcoord_micro: [0, 0],
        },
        SceneVertex {
            position_millimeters: [100, 0, 0],
            normal_micro: [0, 0, 1_000_000],
            tangent_micro: None,
            texcoord_micro: [1_000_000, 0],
        },
        SceneVertex {
            position_millimeters: [0, 100, 10],
            normal_micro: [0, 0, 1_000_000],
            tangent_micro: None,
            texcoord_micro: [0, 1_000_000],
        },
    ];
    let mesh = SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2], None).unwrap(),
    ])
    .unwrap();
    let object = SceneObject::new(
        SceneTransform {
            position_millimeters: [0; 3],
            rotation: SceneRotation::IDENTITY,
            scale_millionths: 1_000_000,
        },
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    let scene = worldrunner_render_plan::RenderScene::new(SceneParts {
        resources: SceneResources::new(
            vec![texture(255), texture(128), texture(255)],
            vec![material],
        )
        .unwrap(),
        geometry: SceneGeometry::new(vec![mesh], vec![object]).unwrap(),
        terrain: Some(terrain),
        effects: None,
    })
    .unwrap();
    PreparedScene::new(&scene).unwrap()
}

#[test]
fn synthetic_heightfield_admission_reports_layout_bytes_and_limits() {
    let limits = wgpu::Limits::default();
    let configuration = crate::compositor::RendererConfiguration::legacy(
        [2, 2],
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    let allocations_before = configuration.allocations().validate(&limits).unwrap();
    let report = configuration
        .opaque_admission_report(&prepared(), &limits)
        .unwrap();
    assert_eq!(
        configuration.allocations().validate(&limits).unwrap(),
        allocations_before
    );
    assert_eq!(
        report.material_families,
        [
            OpaqueMaterialFamily::Terrain,
            OpaqueMaterialFamily::OpaqueMesh
        ]
    );
    assert_eq!(
        report.layouts[0].formats,
        ["Rgba16Float", "Rgba16Float", "Rgba16Float", "Rgba16Uint"]
    );
    assert_eq!(report.layouts[0].bytes_per_sample, 32);
    assert_eq!(
        report.layouts[0].byte_limit,
        limits.max_color_attachment_bytes_per_sample
    );
    assert_eq!(report.layouts[1].bytes_per_sample, 14);
    assert_eq!(report.layouts[0].attachment_count, 4);
    assert_eq!(
        report.layouts[0].attachment_limit,
        limits.max_color_attachments
    );
    assert_eq!(report.layouts[0].record_count, 1);
    assert_eq!(report.layouts[0].record_limit, 65_536);
    assert_eq!(report.layouts[1].record_limit, 65_536);
    assert!(report.layouts[0].admitted);
}

#[test]
fn unsupported_samples_and_record_overflow_are_typed() {
    assert!(matches!(
        admission_report(&prepared(), &wgpu::Limits::default(), 4),
        Err(OpaqueAdmissionError::UnsupportedSampleCount { observed: 4 })
    ));
    assert_eq!(
        validate_record_count(65_537, MaterialIdFormat::Rgba16Uint),
        Err(OpaqueAdmissionError::RecordOverflow {
            records: 65_537,
            limit: 65_536,
        })
    );
}
