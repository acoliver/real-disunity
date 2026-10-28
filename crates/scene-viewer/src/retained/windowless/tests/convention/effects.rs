use super::*;

fn material(roughness_micro: u32) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: (roughness_micro == 100_000).then_some(TextureHandle::from_raw(1)),
        roughness_metallic: (roughness_micro == 100_000).then_some(TextureHandle::from_raw(2)),
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: roughness_micro == 1_000_000,
    })
    .unwrap()
}

fn wall() -> SceneMesh {
    let vertices = [
        [-1000, 500, 0],
        [200, 500, 0],
        [200, 500, 2200],
        [-1000, 500, 2200],
    ]
    .into_iter()
    .map(|position_millimeters| SceneVertex {
        position_millimeters,
        normal_micro: [0, -1_000_000, 0],
        tangent_micro: None,
        texcoord_micro: [0; 2],
    })
    .collect();
    SceneMesh::new(vec![
        SceneMeshLod::new(0, vertices, vec![0, 1, 2, 0, 2, 3], None).unwrap(),
    ])
    .unwrap()
}

fn terrain() -> SceneTerrain {
    let surface = |tint_percent| {
        SceneTerrainSurface::new(
            MaterialHandle::from_raw(0),
            SceneTerrainAppearance {
                tint_percent,
                ..SceneTerrainAppearance::NEUTRAL
            },
        )
    };
    SceneTerrain::new(SceneTerrainParts {
        columns: 5,
        rows: 5,
        origin_east_millimeters: -2000,
        origin_north_millimeters: -2000,
        sample_spacing_millimeters: 1000,
        heights_millimeters: vec![0; 25],
        surface_indices: (0..25).map(|i| u8::from(i % 5 >= 3)).collect(),
        surfaces: vec![surface([0, 100, 0]), surface([0, 0, 100])],
        lod_policy: TerrainLodPolicy::FullResolution,
    })
    .unwrap()
}

fn effects() -> SceneEffects {
    let base = scene();
    let initial = base.effects().unwrap();
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky: *initial.sky(),
        sun: SceneSun {
            direction: SceneDirection::new([0, 707_107, 707_107]).unwrap(),
            irradiance_micro: 3_000_000,
        },
        ambient: SceneAmbient {
            sky_irradiance_micro: 200_000,
            ground_irradiance_micro: 200_000,
        },
        shadows: initial.shadows().clone(),
        camera: *initial.camera(),
        grading: *initial.grading(),
        post: vec![],
        anti_alias: *initial.anti_alias(),
        ssao: Some(SceneSsao {
            radius_millimeters: 600,
            sample_count: 32,
            kernel: SceneSsaoKernel::Hemisphere,
            ..initial.ssao().copied().unwrap()
        }),
    })
    .unwrap()
    .with_ssr(SceneSsr {
        max_steps: 256,
        ..Default::default()
    })
    .unwrap()
}

fn effect_scene() -> RenderScene {
    let base = crate::renderer::retained_tests::terrain_source();
    let mut textures = base.resources().textures().to_vec();
    textures[2] = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![0, 26, 0, 255].into_boxed_slice(),
    };
    let object = SceneObject::new(
        *base.geometry().objects()[0].transform(),
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(1),
    )
    .with_tint(SceneTint::new([1_000_000, 0, 0]).unwrap());
    RenderScene::new(SceneParts {
        resources: SceneResources::new(textures, vec![material(100_000), material(1_000_000)])
            .unwrap(),
        geometry: SceneGeometry::new(vec![wall()], vec![object]).unwrap(),
        terrain: Some(terrain()),
        effects: Some(effects()),
    })
    .unwrap()
}

fn capture_effects(
    shadows: bool,
    handedness: ViewHandedness,
    caster: bool,
) -> RetainedCaptureProducts {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let options = crate::ViewerOptions {
        camera_convention: convention(handedness),
        ..camera_host(shadows.then(crate::renderer::retained_tests::shadow_settings)).options
    };
    let scene = effect_scene();
    let scene = if caster {
        scene
    } else {
        scene
            .with_objects_and_effects(vec![], scene.effects().cloned())
            .unwrap()
    };
    let host = RetainedHost::new(scene, options).unwrap();
    let mut run = host.windowless(&mut driver).unwrap();
    let captured = products(&mut run, &events);
    run.shutdown().unwrap();
    captured
}

fn assert_receivers(
    plain: &RetainedCaptureProducts,
    shadowed: &RetainedCaptureProducts,
    no_caster: &RetainedCaptureProducts,
    mirrored: bool,
) {
    let index = |x: usize, y: usize| y * 64 + if mirrored { 63 - x } else { x };
    let color = &shadowed.linear_hdr.as_ref().unwrap().pixels;
    // With eye (0,-2,2), up=(0,1,1)/sqrt(2), depth=(4+y-z)/sqrt(2):
    // screen=(32+32*sqrt(3)*x/depth, 32-32*sqrt(3)*(y+z)/(sqrt(2)*depth)).
    // Floor pixel (23,42) is near (-.36,-.64,0), inside the wall's -Y cast shadow.
    let receiver = index(23, 42);
    let unshadowed = plain.linear_hdr.as_ref().unwrap().pixels[receiver][1];
    assert!(
        (unshadowed - 0.849).abs() < 0.02,
        "direct sun: {unshadowed}"
    );
    let without_caster = no_caster.linear_hdr.as_ref().unwrap().pixels[receiver][1];
    // Full occlusion leaves only the authored 0.2 ambient radiance.
    assert!((color[receiver][1] - 0.2).abs() < 0.005);
    assert!((without_caster - 0.676).abs() < 0.02);
    assert!(without_caster - color[receiver][1] > 0.45);
    assert!(unshadowed - color[receiver][1] > 0.6);
    // Pixel (25,29) lies at (-.347,.189,0), 311 mm from the wall, inside 600 mm AO.
    // Its reflected eye ray reaches y=.5 at z=.284, inside the red wall.
    let contact = index(25, 29);
    let ao = shadowed.ambient_occlusion.as_ref().unwrap().pixels[contact];
    let confidence = shadowed.ssr_confidence.as_ref().unwrap().pixels[contact];
    assert!((ao - 0.9705).abs() < 0.01, "contact AO: {ao}");
    assert!(
        (confidence - 1.0).abs() < 0.005,
        "known SSR hit: {confidence}"
    );
    assert!(
        color[contact][0] - color[contact][2] > 0.006,
        "red reflected radiance: {:?}",
        color[contact]
    );
    assert!(shadowed.ssr_confidence.as_ref().unwrap().pixels[receiver] < 0.005);
    assert!(no_caster.ssr_confidence.as_ref().unwrap().pixels[contact] < 0.005);
    assert!((no_caster.ambient_occlusion.as_ref().unwrap().pixels[contact] - 1.0).abs() < 0.005);
    // Red mesh center (-.4,.5,1.1) projects near (23,6); its lower landmark
    // (-.4,.5,.55) projects near (24,17). Positive-X terrain blue starts on the right.
    let mesh = color[index(24, 17)];
    assert!((mesh[0] - 0.1945).abs() < 0.015 && mesh[1] < 0.001 && mesh[2] < 0.001);
    let terrain = color[index(48, 32)];
    assert!((terrain[1] - 0.5625).abs() < 0.02 && (terrain[2] - 0.2832).abs() < 0.02);
    assert!(
        color[index(15, 32)][2] < 0.01,
        "blue terrain landmark must be on +X side"
    );
}

#[test]
fn retained_effects_have_visible_receivers_and_independent_landmarks() {
    let captures = [
        ViewHandedness::RightHanded,
        ViewHandedness::DirectXLeftHanded,
    ]
    .map(|handedness| {
        [
            capture_effects(false, handedness, true),
            capture_effects(true, handedness, true),
            capture_effects(true, handedness, false),
        ]
    });
    assert_receivers(&captures[0][0], &captures[0][1], &captures[0][2], false);
    assert_receivers(&captures[1][0], &captures[1][1], &captures[1][2], true);
    for (index, (right, left)) in captures[0].iter().zip(&captures[1]).enumerate() {
        let scene = effect_scene();
        let scene = if index == 2 {
            scene
                .with_objects_and_effects(vec![], scene.effects().cloned())
                .unwrap()
        } else {
            scene
        };
        assert_geometry_mirrored(right, left, &scene);
        assert_effects_mirrored(right, left);
    }
}

fn assert_effects_mirrored(right: &RetainedCaptureProducts, left: &RetainedCaptureProducts) {
    for (a, b) in [
        (
            right.ambient_occlusion.as_ref().unwrap(),
            left.ambient_occlusion.as_ref().unwrap(),
        ),
        (
            right.ssr_confidence.as_ref().unwrap(),
            left.ssr_confidence.as_ref().unwrap(),
        ),
    ] {
        for (x, y) in [(23, 42), (25, 29), (48, 32), (24, 17)] {
            assert!((a.pixels[y * 64 + x] - b.pixels[y * 64 + 63 - x]).abs() < 0.01);
        }
    }
}
