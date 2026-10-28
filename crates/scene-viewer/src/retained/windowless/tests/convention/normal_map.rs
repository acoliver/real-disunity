use super::*;

fn mapped_mesh(base: &RenderScene) -> SceneMesh {
    let mut vertices = base.geometry().meshes()[0].lods()[0].vertices().to_vec();
    for vertex in &mut vertices {
        vertex.texcoord_micro = [
            (vertex.position_millimeters[0] + 1000) * 500,
            (vertex.position_millimeters[1] + 1000) * 500,
        ];
        assert!(vertex.tangent_micro.is_none());
    }
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap()
}

fn mapped_scene() -> RenderScene {
    let base = crate::renderer::retained_tests::source();
    let mesh = mapped_mesh(&base);
    let mut textures = base.resources().textures().to_vec();
    textures.push(SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![191, 159, 255, 255].into_boxed_slice(),
    });
    let material = SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    })
    .unwrap();
    let mut sh = [[0; 3]; 9];
    sh[0] = [1_000_000; 3];
    sh[1][0] = 500_000;
    sh[2][1] = 500_000;
    sh[3][2] = 250_000;
    let effects = scene()
        .effects()
        .unwrap()
        .clone()
        .with_diffuse_environment(SceneDiffuseEnvironment::Sh9(SceneSh9::new(sh).unwrap()));
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![material]).unwrap(),
        geometry: SceneGeometry::new(vec![mesh], base.geometry().objects().to_vec()).unwrap(),
        terrain: None,
        effects: Some(effects),
    })
    .unwrap()
}

#[test]
fn derivative_normal_map_has_independent_normal_and_lighting_oracles() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let host = RetainedHost::new(mapped_scene(), camera_host(None).options).unwrap();
    let mut run = host.windowless(&mut driver).unwrap();
    // For +Z, U=+X, V=+Y and downward raster Y, the derivative basis is (-X,-Y,Z).
    let nx = -127.0f32 / 255.0;
    let ny = -63.0f32 / 255.0;
    let nz = (1.0 - nx * nx - ny * ny).sqrt();
    let expected = [
        nx,
        (ny + nz) / 2.0f32.sqrt(),
        (nz - ny) / 2.0f32.sqrt(),
        1.0,
    ];
    let lighting = [1.0 + 0.5 * nx, 1.0 + 0.5 * ny, 1.0 + 0.25 * nz];
    for (handedness, x) in [
        (ViewHandedness::RightHanded, 32),
        (ViewHandedness::DirectXLeftHanded, 31),
    ] {
        set_camera(&mut run, handedness);
        let captured = products(&mut run, &events);
        let pixel = 32 * 64 + x;
        for (actual, expected) in captured.normals.unwrap().pixels[pixel]
            .into_iter()
            .zip(expected)
        {
            assert!(
                (actual - expected).abs() < 0.002,
                "normal {actual} != {expected}"
            );
        }
        for (actual, expected) in captured.linear_hdr.unwrap().pixels[pixel][..3]
            .iter()
            .zip(lighting)
        {
            assert!(
                (actual - expected).abs() < 0.003,
                "lighting {actual} != {expected}"
            );
        }
    }
    run.shutdown().unwrap();
}
