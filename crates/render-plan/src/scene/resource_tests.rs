use super::*;

fn texels(bytes: &[u8]) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Srgb,
        width: 2,
        height: 2,
        mips: 1,
        bytes: bytes.to_vec().into_boxed_slice(),
    }
}

#[test]
fn texel_payload_must_satisfy_exact_level_arithmetic() {
    assert!(texels(&[0; 16]).validate().is_ok());
    assert!(matches!(
        texels(&[0; 15]).validate(),
        Err(SceneValidationError::ByteLengthMismatch {
            expected: 16,
            observed: 15,
            ..
        })
    ));
    assert!(matches!(
        texels(&[0; 28]).validate(),
        Err(SceneValidationError::ByteLengthMismatch {
            expected: 16,
            observed: 28,
            ..
        })
    ));
}

#[test]
fn mip_chain_includes_every_level() {
    let texture = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 2,
        bytes: vec![0; 5].into_boxed_slice(),
    };
    assert!(texture.validate().is_ok());
    let too_many_mips = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 2,
        height: 2,
        mips: 3,
        bytes: vec![0; 5].into_boxed_slice(),
    };
    assert!(matches!(
        too_many_mips.validate(),
        Err(SceneValidationError::MipOverflow {
            observed: 3,
            maximum: 2,
            ..
        })
    ));
}

fn npot_rgba(width: u32, height: u32, mips: u32, bytes: usize) -> SceneTexture {
    SceneTexture::Texels {
        format: TexelFormat::Rgba8Unorm,
        color_space: TextureColorSpace::Linear,
        width,
        height,
        mips,
        bytes: vec![0; bytes].into_boxed_slice(),
    }
}

#[test]
fn npot_rgba_mips_validate_directly_and_through_scene_resources() {
    // 5x3 -> 2x1 -> 1x1; a partial chain stops at 2x1.
    // A one-texel axis stays one: 1x5 -> 1x2 -> 1x1.
    for texture in [
        npot_rgba(5, 3, 3, 72),
        npot_rgba(5, 3, 2, 68),
        npot_rgba(1, 5, 3, 32),
    ] {
        assert_eq!(texture.validate(), Ok(()), "{texture:?}");
        let resources = SceneResources::new(vec![texture.clone()], vec![])
            .expect("floor-halved NPOT mip payloads must be accepted");
        assert_eq!(resources.textures(), &[texture]);
        assert_eq!(resources.validate(), Ok(()));
    }
}

#[test]
fn npot_rgba_rejects_inexact_and_old_ceil_payloads_at_both_boundaries() {
    for observed in [71, 73, 92] {
        let texture = npot_rgba(5, 3, 3, observed);
        let expected = Err(SceneValidationError::ByteLengthMismatch {
            part: "texture texels",
            reason: "texel payload must equal the concatenated level sizes",
            expected: 72,
            observed: u64::try_from(observed).expect("fixture length fits"),
        });
        assert_eq!(texture.validate(), expected, "{observed} bytes, direct");
        assert_eq!(
            SceneResources::new(vec![texture], vec![]).map(|_| ()),
            expected,
            "{observed} bytes, resources"
        );
    }
}

#[test]
fn npot_rgba_rejects_zero_and_excess_mips_at_both_boundaries() {
    for (width, height) in [(5, 3), (1, 5)] {
        for observed in [0, 4] {
            let texture = npot_rgba(width, height, observed, 0);
            let expected = Err(SceneValidationError::MipOverflow {
                part: "texture texels",
                observed,
                maximum: 3,
            });
            assert_eq!(texture.validate(), expected, "{width}x{height}, direct");
            assert_eq!(
                SceneResources::new(vec![texture], vec![]).map(|_| ()),
                expected,
                "{width}x{height}, resources"
            );
        }
    }
}

#[test]
fn zero_byte_payload_survives_only_where_the_format_permits_it() {
    let opaque = SceneTexture::Encoded {
        encoding: TextureEncoding::OpaqueExact,
        color_space: TextureColorSpace::Linear,
        bytes: Vec::new().into_boxed_slice(),
    };
    assert!(opaque.validate().is_ok());
    let container = SceneTexture::Encoded {
        encoding: TextureEncoding::DdsContainer,
        color_space: TextureColorSpace::Srgb,
        bytes: Vec::new().into_boxed_slice(),
    };
    assert!(matches!(
        container.validate(),
        Err(SceneValidationError::EmptyEncodedPayload { .. })
    ));
}

/// Builds an opaque material addressing only the mandatory albedo channel.
fn material_parts() -> SceneMaterialParts {
    SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 0,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    }
}

#[test]
fn material_handles_and_scalars_are_validated() {
    let material = SceneMaterial::new(SceneMaterialParts {
        normal: Some(TextureHandle::from_raw(1)),
        ..material_parts()
    })
    .expect("in-range scalars validate");
    let resources = SceneResources::new(vec![texels(&[0; 16]), texels(&[0; 16])], vec![material])
        .expect("referenced textures exist");
    assert_eq!(resources.materials().len(), 1);
    assert!(matches!(
        SceneResources::new(vec![texels(&[0; 16])], vec![material]),
        Err(SceneValidationError::HandleOutOfRange { handle: 1, .. })
    ));
    assert!(matches!(
        SceneMaterial::new(SceneMaterialParts {
            roughness_micro: 1_000_001,
            ..material_parts()
        }),
        Err(SceneValidationError::RangeViolation {
            observed: 1_000_001,
            ..
        })
    ));
}

#[test]
fn every_optional_material_channel_is_range_checked() {
    for (label, parts) in [
        (
            "roughness_metallic",
            SceneMaterialParts {
                roughness_metallic: Some(TextureHandle::from_raw(1)),
                ..material_parts()
            },
        ),
        (
            "ambient_occlusion",
            SceneMaterialParts {
                ambient_occlusion: Some(TextureHandle::from_raw(1)),
                ..material_parts()
            },
        ),
        (
            "self_illumination",
            SceneMaterialParts {
                self_illumination: Some(TextureHandle::from_raw(1)),
                ..material_parts()
            },
        ),
    ] {
        let material = SceneMaterial::new(parts).expect("in-range scalars validate");
        assert!(
            matches!(
                SceneResources::new(vec![texels(&[0; 16])], vec![material]),
                Err(SceneValidationError::HandleOutOfRange { handle: 1, .. })
            ),
            "{label} must be range-checked against the texture table"
        );
    }
}

#[test]
fn a_masked_material_carries_its_own_cutoff() {
    let material = SceneMaterial::new(SceneMaterialParts {
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: 501_961,
        },
        double_sided: true,
        ..material_parts()
    })
    .expect("an in-range cutoff validates");
    assert_eq!(
        material.alpha_mode(),
        SceneAlphaMode::Mask {
            cutoff_micro: 501_961
        }
    );
    assert!(material.double_sided());
    assert!(matches!(
        SceneMaterial::new(SceneMaterialParts {
            alpha_mode: SceneAlphaMode::Mask {
                cutoff_micro: 1_000_001
            },
            ..material_parts()
        }),
        Err(SceneValidationError::RangeViolation {
            observed: 1_000_001,
            ..
        })
    ));
}

#[test]
fn alpha_factor_is_unit_by_default_and_rejects_out_of_range_inputs() {
    let material = SceneMaterial::new(material_parts()).unwrap();
    assert_eq!(material.alpha_factor_micro(), 1_000_000);
    for factor in [0, 333_333, 1_000_000] {
        assert_eq!(
            material
                .with_alpha_factor(factor)
                .unwrap()
                .alpha_factor_micro(),
            factor
        );
    }
    for factor in [1_000_001, u32::MAX] {
        assert_eq!(
            material.with_alpha_factor(factor),
            Err(SceneValidationError::RangeViolation {
                part: "material alpha factor",
                minimum: 0,
                maximum: 1_000_000,
                observed: u64::from(factor),
            })
        );
    }
}

#[test]
fn threshold_policy_validates_texture_shape_format_and_handles() {
    use crate::{SceneFadeCoverage, SceneFadeQuantization};
    let material = SceneMaterial::new(material_parts()).unwrap();
    assert_eq!(material.fade_coverage(), SceneFadeCoverage::Hashed);
    assert_eq!(
        material.fade_quantization(),
        SceneFadeQuantization::Continuous
    );
    let threshold = material.with_fade_coverage(SceneFadeCoverage::Threshold {
        texture: TextureHandle::from_raw(1),
        jitter_pixels: [i32::MIN, i32::MAX],
    });
    assert!(SceneResources::new(vec![texels(&[0; 16])], vec![threshold]).is_err());
    for (format, color_space, width, height, mips, bytes) in [
        (TexelFormat::R8Unorm, TextureColorSpace::Linear, 2, 2, 1, 4),
        (TexelFormat::R8Unorm, TextureColorSpace::Srgb, 2, 2, 1, 4),
        (TexelFormat::Rg8Unorm, TextureColorSpace::Linear, 2, 2, 1, 8),
        (TexelFormat::R8Unorm, TextureColorSpace::Linear, 2, 1, 1, 2),
        (TexelFormat::R8Unorm, TextureColorSpace::Linear, 2, 2, 2, 5),
    ] {
        let texture = SceneTexture::Texels {
            format,
            color_space,
            width,
            height,
            mips,
            bytes: vec![0; bytes].into(),
        };
        let expected = format == TexelFormat::R8Unorm
            && color_space == TextureColorSpace::Linear
            && width == height
            && mips == 1;
        assert_eq!(
            SceneResources::new(vec![texels(&[0; 16]), texture], vec![threshold]).is_ok(),
            expected
        );
    }
}

#[test]
fn zero_contact_distance_retains_a_valid_normal_fold() {
    let contact = crate::SceneTerrainContact::new(crate::SceneTint::WHITE, 123_456, 654_321, 0)
        .unwrap()
        .with_normal_fold(2000, 0)
        .unwrap();
    assert_eq!(contact.distance_millimeters(), 0);
    assert_eq!(contact.normal_fold(), Some([2000, 0]));
    assert_eq!(contact.roughness_micro(), 123_456);
    assert_eq!(contact.metalness_micro(), 654_321);
}
