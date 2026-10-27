//! Behavioural claims about deriving pipeline state from governed materials.

use super::{MATERIAL_UNIFORM_BYTES, MaterialUniform, PipelineKey};
use crate::shader;
use std::collections::BTreeSet;
use wgpu::Face;
use worldrunner_render_plan::{SceneAlphaMode, SceneMaterial, SceneMaterialParts, TextureHandle};

const COMMON_CUTOFF_MICRO: u32 = 501_961;

/// A second authored cutoff, appearing on 57 slots.
const SECOND_CUTOFF_MICRO: u32 = 392_157;

fn material(alpha_mode: SceneAlphaMode, double_sided: bool) -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 1_000_000,
        alpha_mode,
        double_sided,
    })
    .expect("the fixture material carries in-range scalars")
}

#[test]
fn alpha_factor_validates_and_preserves_default_uniform_bytes() {
    let base = material(SceneAlphaMode::Opaque, false);
    assert_eq!(base.alpha_factor_micro(), 1_000_000);
    assert!(base.with_alpha_factor(1_000_001).is_err());
    assert!(base.with_alpha_factor(u32::MAX).is_err());
    let stock = MaterialUniform::from_material(&base).to_bytes();
    assert_eq!(&stock[320..336], &[0; 16]);
    let unit = base.with_alpha_factor(1_000_000).unwrap();
    assert_eq!(stock, MaterialUniform::from_material(&unit).to_bytes());
    for (micro, expected) in [(0, 0.0_f32), (500_000, 0.5)] {
        let material = base.with_alpha_factor(micro).unwrap();
        let bytes = MaterialUniform::from_material(&material).to_bytes();
        assert_eq!(&bytes[..320], &stock[..320]);
        assert_eq!(&bytes[328..], &stock[328..]);
        assert_eq!(&bytes[320..324], &expected.to_le_bytes());
        assert_eq!(&bytes[324..328], &1.0_f32.to_le_bytes());
    }
}

#[test]
fn a_single_sided_material_culls_back_faces() {
    let key = PipelineKey::from_material(&material(SceneAlphaMode::Opaque, false));
    assert_eq!(key.cull_mode(), Some(Face::Back));
}

#[test]
fn a_double_sided_material_culls_nothing() {
    let key = PipelineKey::from_material(&material(SceneAlphaMode::Opaque, true));
    assert_eq!(key.cull_mode(), None);
}

#[test]
fn a_double_sided_material_flips_its_back_face_normals() {
    // The authored normal points out of the front face only. Left uncorrected,
    // the lit and unlit sides of every leaf swap as the camera orbits.
    let double = PipelineKey::from_material(&material(SceneAlphaMode::Opaque, true));
    let single = PipelineKey::from_material(&material(SceneAlphaMode::Opaque, false));
    assert!(double.flips_back_face_normals());
    assert!(!single.flips_back_face_normals());
}

#[test]
fn a_masked_material_selects_the_discarding_variant() {
    let key = PipelineKey::from_material(&material(
        SceneAlphaMode::Mask {
            cutoff_micro: COMMON_CUTOFF_MICRO,
        },
        false,
    ));
    assert!(key.masked);
}

#[test]
fn an_opaque_material_selects_a_variant_that_never_discards() {
    // Discarding defeats early depth rejection, so the 190 opaque slots must
    // not pay for a test they never fail.
    let key = PipelineKey::from_material(&material(SceneAlphaMode::Opaque, false));
    assert!(!key.masked);
}

#[test]
fn the_cutoff_value_does_not_change_the_pipeline() {
    let common = PipelineKey::from_material(&material(
        SceneAlphaMode::Mask {
            cutoff_micro: COMMON_CUTOFF_MICRO,
        },
        true,
    ));
    let second = PipelineKey::from_material(&material(
        SceneAlphaMode::Mask {
            cutoff_micro: SECOND_CUTOFF_MICRO,
        },
        true,
    ));
    assert_eq!(common, second);
}

#[test]
fn the_cutoff_value_does_reach_the_shader() {
    // Having established the cutoff is not pipeline state, it must still get
    // through, or masking would silently use one arbitrary threshold for every
    // material in the scene.
    let uniform = MaterialUniform::from_material(&material(
        SceneAlphaMode::Mask {
            cutoff_micro: SECOND_CUTOFF_MICRO,
        },
        true,
    ));
    assert_eq!(uniform.alpha_cutoff_micro, SECOND_CUTOFF_MICRO);
}

#[test]
fn an_opaque_material_reports_no_cutoff() {
    let uniform = MaterialUniform::from_material(&material(SceneAlphaMode::Opaque, false));
    assert_eq!(uniform.alpha_cutoff_micro, 0);
}

#[test]
fn every_reachable_material_maps_into_the_prebuilt_pipeline_set() {
    // Pipelines are built eagerly, so the set must be exhaustive: a material
    // that mapped outside it would fail at draw time, in the middle of a
    // frame, rather than at startup.
    let all: BTreeSet<PipelineKey> = PipelineKey::ALL.into_iter().collect();
    assert_eq!(all.len(), 8, "the four variants are distinct");
    for double_sided in [false, true] {
        for alpha_mode in [
            SceneAlphaMode::Opaque,
            SceneAlphaMode::Mask {
                cutoff_micro: COMMON_CUTOFF_MICRO,
            },
        ] {
            let key = PipelineKey::from_material(&material(alpha_mode, double_sided));
            assert!(all.contains(&key), "{key:?} must be prebuilt");
        }
    }
}

#[test]
fn every_key_indexes_a_slot_of_its_own() {
    // Callers hold the four pipelines in an array built by mapping over `ALL`,
    // so this arithmetic and that order are two descriptions of one layout.
    // Nothing downstream can catch them parting: every pipeline accepts every
    // material's buffers, so a misindexed draw is not an error, just a wrong
    // picture.
    for (position, key) in PipelineKey::ALL.iter().enumerate() {
        assert_eq!(key.index(), position, "{key:?} must index its own slot");
    }

    let slots: BTreeSet<usize> = PipelineKey::ALL.iter().map(|key| key.index()).collect();
    assert_eq!(
        slots.len(),
        PipelineKey::ALL.len(),
        "no two states may collapse onto one slot"
    );
}

#[test]
fn the_scalar_factors_reach_the_shader_unmodified() {
    let uniform = MaterialUniform::from_material(&material(SceneAlphaMode::Opaque, false));
    assert_eq!(uniform.roughness_micro, 1_000_000);
    assert_eq!(uniform.metalness_micro, 1_000_000);
}

#[test]
fn a_material_declares_which_optional_channels_it_binds() {
    // An unbound texture slot samples as opaque black. Without a presence flag
    // an absent ambient-occlusion map would darken the surface to nothing,
    // which reads as a lighting bug rather than a missing texture.
    let bare = MaterialUniform::from_material(&material(SceneAlphaMode::Opaque, false));
    assert_eq!(bare.texture_flags, 0);

    let full = MaterialUniform::from_material(&fully_textured());
    assert_eq!(
        full.texture_flags,
        shader::FLAG_NORMAL
            | shader::FLAG_ROUGH_METAL
            | shader::FLAG_AMBIENT_OCCLUSION
            | shader::FLAG_SELF_ILLUMINATION
    );
}

#[test]
fn each_optional_channel_sets_only_its_own_flag() {
    let mut parts = bare_parts();
    parts.roughness_metallic = Some(TextureHandle::from_raw(7));
    let material = SceneMaterial::new(parts).expect("the fixture material is valid");
    let uniform = MaterialUniform::from_material(&material);
    assert_eq!(
        uniform.texture_flags,
        shader::FLAG_ROUGH_METAL,
        "binding one channel must not imply any other"
    );
}

#[test]
fn the_uniform_encodes_the_layout_the_shader_declares() {
    // The shader reads four 4-byte fields in this order. Encoding is by hand
    // because the workspace forbids unsafe, so nothing checks this but a test.
    let material = SceneMaterial::new(SceneMaterialParts {
        alpha_mode: SceneAlphaMode::Mask {
            cutoff_micro: COMMON_CUTOFF_MICRO,
        },
        roughness_micro: 250_000,
        metalness_micro: 500_000,
        ..bare_parts()
    })
    .expect("the fixture material is valid");
    let bytes = MaterialUniform::from_material(&material).to_bytes();
    assert_eq!(bytes.len(), MATERIAL_UNIFORM_BYTES);
    assert!(
        (f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) - 0.501_961).abs() < 1e-6,
        "the cutoff must arrive as a unit fraction, not micro units"
    );
    assert!((f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) - 0.25).abs() < 1e-6);
    assert!((f32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) - 0.5).abs() < 1e-6);
    assert_eq!(
        u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        0
    );
}

#[test]
fn an_unmasked_material_carries_a_cutoff_that_discards_nothing() {
    // The opaque pipeline never runs the test, but a stray non-zero cutoff
    // reaching a masked pipeline would erase geometry, so zero is the safe rest
    // state rather than an accident.
    let uniform = MaterialUniform::from_material(&material(SceneAlphaMode::Opaque, false));
    assert_eq!(uniform.alpha_cutoff_micro, 0);
}

fn bare_parts() -> SceneMaterialParts {
    SceneMaterialParts {
        albedo: TextureHandle::from_raw(0),
        normal: None,
        roughness_metallic: None,
        ambient_occlusion: None,
        self_illumination: None,
        roughness_micro: 1_000_000,
        metalness_micro: 1_000_000,
        alpha_mode: SceneAlphaMode::Opaque,
        double_sided: false,
    }
}

fn fully_textured() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
        normal: Some(TextureHandle::from_raw(1)),
        roughness_metallic: Some(TextureHandle::from_raw(2)),
        ambient_occlusion: Some(TextureHandle::from_raw(3)),
        self_illumination: Some(TextureHandle::from_raw(4)),
        ..bare_parts()
    })
    .expect("the fixture material is valid")
}

#[test]
fn normal_parameters_are_validated_and_encoded_without_changing_stock_prefix() {
    use worldrunner_render_plan::{SceneNormalEncoding, SceneNormalY};
    let base = material(SceneAlphaMode::Opaque, false);
    assert_eq!(base.normal_encoding(), SceneNormalEncoding::TwoChannelXy);
    assert_eq!(base.normal_y(), SceneNormalY::Up);
    assert!(base.with_normal_scale(4_000_001).is_err());
    let stock = MaterialUniform::from_material(&base).to_bytes();
    for micro in [0, 1_000_000, 2_000_000, 4_000_000] {
        let changed = base
            .with_normal_scale(micro)
            .unwrap()
            .with_normal_encoding(SceneNormalEncoding::Rgb, SceneNormalY::Down);
        let bytes = MaterialUniform::from_material(&changed).to_bytes();
        assert_eq!(&bytes[..72], &stock[..72]);
        assert_eq!(&bytes[76..80], &3_u32.to_le_bytes());
        assert_eq!(
            &bytes[72..76],
            &f32::from(u16::try_from(micro / 1_000_000).unwrap()).to_le_bytes()
        );
    }
}
