#[test]
fn environment_lut_requires_linear_float_square_single_mip() {
    for (format, color_space, width, height, mips) in [
        (TexelFormat::Rgba8Unorm, TextureColorSpace::Linear, 1, 1, 1),
        (TexelFormat::Rg16Float, TextureColorSpace::Srgb, 1, 1, 1),
        (TexelFormat::Rg16Float, TextureColorSpace::Linear, 2, 1, 1),
        (TexelFormat::Rg16Float, TextureColorSpace::Linear, 2, 2, 2),
    ] {
        let mut textures = resources().textures().to_vec();
        let texels = if mips == 2 { 5 } else { width * height };
        textures[1] = SceneTexture::Texels {
            format,
            color_space,
            width,
            height,
            mips,
            bytes: vec![0; usize::try_from(u64::from(texels) * format.bytes_per_texel()).unwrap()]
                .into(),
        };
        let resources = SceneResources::new(textures, vec![]).unwrap();
        assert!(matches!(
            environment().validate(&resources, SceneSpecularConvention::default()),
            Err(SceneValidationError::UnsupportedInput {
                part: "environment BRDF LUT format"
            })
        ));
    }
    assert!(matches!(
        SceneEnvironment {
            cube: TextureHandle::from_raw(99),
            ..environment()
        }
        .validate(&resources(), SceneSpecularConvention::default()),
        Err(SceneValidationError::HandleOutOfRange {
            handle: 99,
            limit: 2,
            ..
        })
    ));
}

use worldrunner_render_plan::*;

fn resources() -> SceneResources {
    SceneResources::new(
        vec![
            SceneTexture::SpecularCube(
                SceneSpecularCube::rgba16_float(1, 1, vec![0; 48].into()).unwrap(),
            ),
            SceneTexture::Texels {
                format: TexelFormat::Rg16Float,
                color_space: TextureColorSpace::Linear,
                width: 1,
                height: 1,
                mips: 1,
                bytes: vec![0; 4].into(),
            },
        ],
        vec![],
    )
    .unwrap()
}

fn environment() -> SceneEnvironment {
    SceneEnvironment {
        cube: TextureHandle::from_raw(0),
        brdf_lut: TextureHandle::from_raw(1),
        convention: SceneSpecularConvention::default(),
        consumers: SceneEnvironmentConsumers::Both,
    }
}

#[test]
fn environment_references_and_formats_fail_before_upload() {
    let resources = resources();
    let convention = SceneSpecularConvention::default();
    assert!(environment().validate(&resources, convention).is_ok());
    for (cube, lut) in [(2, 1), (0, 2), (1, 1), (0, 0)] {
        let invalid = SceneEnvironment {
            cube: TextureHandle::from_raw(cube),
            brdf_lut: TextureHandle::from_raw(lut),
            ..environment()
        };
        assert!(invalid.validate(&resources, convention).is_err());
    }
    let mismatch = SceneSpecularConvention {
        visibility: SceneSpecularVisibility::SchlickSmith,
        ..convention
    };
    assert!(environment().validate(&resources, mismatch).is_err());
}
