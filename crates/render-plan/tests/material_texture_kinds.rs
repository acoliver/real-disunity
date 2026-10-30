use worldrunner_render_plan::*;

fn parts() -> SceneMaterialParts {
    SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 500_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    }
}

fn materials() -> Vec<SceneMaterial> {
    let cube = TextureHandle::from_raw(1);
    let base = SceneMaterial::new(parts()).unwrap();
    let mut materials = vec![
        SceneMaterial::new(SceneMaterialParts {
            albedo: cube,
            ..parts()
        })
        .unwrap(),
        SceneMaterial::new(SceneMaterialParts {
            normal: Some(cube),
            ..parts()
        })
        .unwrap(),
        SceneMaterial::new(SceneMaterialParts {
            roughness_metallic: Some(cube),
            ..parts()
        })
        .unwrap(),
        SceneMaterial::new(SceneMaterialParts {
            ambient_occlusion: Some(cube),
            ..parts()
        })
        .unwrap(),
        SceneMaterial::new(SceneMaterialParts {
            self_illumination: Some(cube),
            ..parts()
        })
        .unwrap(),
    ];
    let atlas = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [2, 2], 0, true).unwrap();
    materials.push(base.with_impostor(atlas.with_relief(cube, 1).unwrap()));
    materials.push(base.with_impostor(atlas.with_borders(cube)));
    materials.push(base.with_fade_coverage(SceneFadeCoverage::Threshold {
        texture: cube,
        jitter_pixels: [0; 2],
    }));
    let wind = SceneWind::new(SceneDirection::new([1_000_000, 0, 0]).unwrap(), 1, 1, 0)
        .unwrap()
        .with_field(ScenePlanarWindField::new(cube, [0; 2], [1; 2], [None; 2]).unwrap());
    materials.push(
        base.with_features(SceneMaterialFeatures {
            tint: SceneTint::WHITE,
            ambient_occlusion: SceneAmbientOcclusion::UNOCCLUDED,
            wind: Some(wind),
            layer: None,
            translucency: None,
        })
        .unwrap(),
    );
    materials
}

fn rejects_cube(sampled: bool) {
    let cube = SceneTexture::SpecularCube(
        SceneSpecularCube::rgba16_float(1, 1, vec![0; 48].into()).unwrap(),
    );
    let cube = if sampled {
        cube.with_sampler(TextureSampler::default())
    } else {
        cube
    };
    let albedo = SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: vec![255; 4].into(),
    };
    assert!(SceneResources::new(vec![albedo.clone(), cube.clone()], vec![]).is_ok());
    for (slot, material) in materials().into_iter().enumerate() {
        assert!(
            matches!(
                SceneResources::new(vec![albedo.clone(), cube.clone()], vec![material]),
                Err(SceneValidationError::UnsupportedInput { .. })
            ),
            "material texture slot {slot}, sampled={sampled}"
        );
    }
}

#[test]
fn material_slots_reject_direct_specular_cubes() {
    rejects_cube(false);
}

#[test]
fn material_slots_reject_sampled_specular_cubes() {
    rejects_cube(true);
}
