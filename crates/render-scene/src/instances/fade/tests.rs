use super::*;
use worldrunner_render_plan::{
    MaterialHandle, MeshHandle, SceneAlphaMode, SceneFadeCoverage, SceneImpostor,
    SceneImpostorLayout, SceneMaterialParts, SceneRotation, SceneTransform, TexelFormat,
    TextureColorSpace, TextureHandle,
};

/// A texture table whose threshold handles (0 and 1) hold one 16x16 R8 grid with every byte
/// value, the thresholds the shader oracle below exercises. Its zero texel keeps zero opacity.
fn every_threshold() -> Vec<SceneTexture> {
    let grid = SceneTexture::Texels {
        format: TexelFormat::R8Unorm,
        color_space: TextureColorSpace::Linear,
        width: 16,
        height: 16,
        mips: 1,
        bytes: (0..=255_u8).collect::<Vec<_>>().into(),
    };
    vec![grid.clone(), grid]
}

fn material() -> SceneMaterial {
    SceneMaterial::new(SceneMaterialParts {
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
    .unwrap()
}

// Literal scalar emulation of geometry.wgsl, independent of the culling intervals.
fn shader_factor(distance: f32, band: [f32; 2]) -> f32 {
    let [start, end] = band;
    if start < 0.0 {
        return f32::from((distance <= end) == (start.to_bits() == (-2.0_f32).to_bits()));
    }
    if end < 0.0 {
        if distance <= start {
            return 1.0;
        }
        return ((-end - distance) / (-end - start).max(1.0)).max(0.0);
    }
    if distance < start {
        return 0.0;
    }
    if distance > end {
        return 1.0;
    }
    1.0 - (end - distance) / (end - start).max(1.0)
}

fn shader_empty(material: &SceneMaterial, band: [f32; 2], distance: f32) -> bool {
    let direction = material.fade_direction();
    let mut band = band;
    if direction == SceneFadeDirection::Complement {
        band = if band[0] < 0.0 {
            [
                if band[0].to_bits() == (-2.0_f32).to_bits() {
                    -1.0
                } else {
                    -2.0
                },
                band[1],
            ]
        } else {
            [band[0], -band[1]]
        };
    }
    let mut opacity = shader_factor(distance, band);
    if material.fade_quantization() == SceneFadeQuantization::Byte {
        opacity = (opacity * 255.0).trunc() / 255.0;
    }
    // Exercise all R8 thresholds and the extremes of the hashed range.
    let thresholds: Vec<_> = if matches!(material.fade_coverage(), SceneFadeCoverage::Hashed) {
        vec![0.5 / 65536.0, 65535.5 / 65536.0]
    } else {
        (0_u8..=255).map(|v| f32::from(v) / 255.0).collect()
    };
    thresholds.into_iter().all(|threshold| !match direction {
        SceneFadeDirection::Inclusive => opacity >= threshold,
        SceneFadeDirection::Keep => threshold < opacity,
        SceneFadeDirection::Complement => threshold >= opacity,
    })
}

fn assert_policy(mat: &SceneMaterial) -> usize {
    let policy = Fade::new(mat, &every_threshold());
    let mut culled = 0;
    for band in [
        [2.0_f32, 6.0],
        [2.0, -6.0],
        [-1.0, 0.0],
        [-2.0, 0.0],
        [-1.0, 6.0],
        [-2.0, 6.0],
        [7.0, 7.5],
        [7.0, -7.5],
    ] {
        let mut distances = vec![0.0, 1.0, 8.0];
        for endpoint in [band[0].max(0.0), band[1].abs()] {
            distances.extend([endpoint.next_down().max(0.0), endpoint, endpoint.next_up()]);
        }
        for step in 0_u16..=255 {
            let d = 6.0 - f32::from(step) * 4.0 / 255.0;
            distances.extend([d.next_down(), d, d.next_up()]);
        }
        for distance in distances {
            let bounded = distance_coverage(Span::point(distance), band);
            let scalar = shader_factor(distance, band);
            assert!(
                bounded.lo <= scalar && bounded.hi >= scalar,
                "{band:?} {distance}: {bounded:?} versus {scalar}"
            );
            let empty = policy.empty_at(Span::point(distance), band);
            let expected = shader_empty(mat, band, distance);
            assert!(!empty || expected, "{mat:?}, {band:?}, {distance:?}");
            if !(1.0..=8.0).contains(&distance) {
                assert_eq!(empty, expected);
            }
            culled += usize::from(empty);
        }
    }
    culled
}

#[test]
fn scalar_fade_boundaries_steps_and_quantization_never_cull_a_shader_survivor() {
    let mut culled = 0;
    for direction in [
        SceneFadeDirection::Inclusive,
        SceneFadeDirection::Keep,
        SceneFadeDirection::Complement,
    ] {
        for quantization in [
            SceneFadeQuantization::Continuous,
            SceneFadeQuantization::Byte,
        ] {
            for threshold in [false, true] {
                let mut mat = material()
                    .with_fade_direction(direction)
                    .with_fade_quantization(quantization);
                if threshold {
                    mat = mat.with_fade_coverage(SceneFadeCoverage::Threshold {
                        texture: TextureHandle::from_raw(1),
                        jitter_pixels: [-3, 7],
                    });
                }
                culled += assert_policy(&mat);
            }
        }
    }
    assert!(culled > 1000);
}

#[test]
fn transformed_reference_clamp_and_instance_override_follow_uploaded_values() {
    let transform = SceneTransform {
        position_millimeters: [1000, 2000, 500],
        rotation: SceneRotation {
            x: 0,
            y: 0,
            z: 1_000_000,
            w: 0,
        },
        scale_millionths: 2_000_000,
    };
    let reference = SceneFadeReference::new([1000, 2000, 1000]);
    let mat = material()
        .with_fade_reference(reference)
        .with_fade_direction(SceneFadeDirection::Keep);
    let band = SceneFadeBand::new(2000, 6000).unwrap().with_terminal_fade();
    let object = SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    )
    .with_fade(band);
    // Transformed reference is (-1,-2,2.5), unlike the placement pivot.
    let policy = Fade::new(&mat, &every_threshold());
    assert!(!policy.empty(object, Vec3::new(-1.0, -2.0, 2.5)));
    assert!(policy.empty(object, Vec3::new(-1.0, -2.0, -4.0)));
    let clamped = Fade::new(
        &mat.with_fade_reference(reference.with_eye_vertical_clamp(0).unwrap()),
        &every_threshold(),
    );
    assert!(!clamped.empty(object, Vec3::new(-1.0, -2.0, -4.0)));
    let impostor = SceneImpostor::new(SceneImpostorLayout::AzimuthElevation, [1, 1], 0, false)
        .unwrap()
        .with_fade(band);
    let policy = Fade::new(&mat.with_impostor(impostor), &every_threshold());
    let bare = SceneObject::new(
        transform,
        MeshHandle::from_raw(0),
        MaterialHandle::from_raw(0),
    );
    assert!(policy.empty(bare, Vec3::new(-1.0, -2.0, -4.0)));
    assert!(
        !policy.empty(
            bare.with_fade(
                SceneFadeBand::new(20_000, 60_000)
                    .unwrap()
                    .with_terminal_fade()
            ),
            Vec3::new(-1.0, -2.0, -4.0)
        )
    );
}

#[test]
fn zero_and_one_threshold_texels_keep_their_equality_owners() {
    let mat = material().with_fade_coverage(SceneFadeCoverage::Threshold {
        texture: TextureHandle::from_raw(1),
        jitter_pixels: [0; 2],
    });
    assert!(!Fade::new(&mat, &every_threshold()).empty_at(Span::point(8.0), [2.0, -6.0]));
    assert!(
        Fade::new(
            &mat.with_fade_direction(SceneFadeDirection::Keep),
            &every_threshold()
        )
        .empty_at(Span::point(8.0), [2.0, -6.0])
    );
    assert!(
        !Fade::new(
            &mat.with_fade_direction(SceneFadeDirection::Complement),
            &every_threshold()
        )
        .empty_at(Span::point(8.0), [2.0, 6.0])
    );
}
