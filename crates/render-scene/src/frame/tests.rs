//! Behavioural claims about the per-frame uniform block.

use super::{FRAME_UNIFORM_BYTES, FrameError, FrameUniform};
use crate::view::{CameraPose, SceneView, Viewport};
use glam::Vec3;
use worldrunner_render_plan::{
    ROTATION_MICRO_SCALE, SceneAmbient, SceneAntiAlias, SceneCamera, SceneColor, SceneColorGrading,
    SceneEffects, SceneEffectsParts, SceneRotation, SceneShadowCascades, SceneSky, SceneSsao,
    SceneSun,
};

/// Reads one little-endian `f32` back out of the encoded block.
fn read_f32(bytes: &[u8; FRAME_UNIFORM_BYTES], offset: usize) -> f32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[offset..offset + 4]);
    f32::from_le_bytes(word)
}

/// A direction pointing due east, expressed the way the domain stores it.
fn east() -> SceneRotation {
    SceneRotation {
        x: ROTATION_MICRO_SCALE,
        y: 0,
        z: 0,
        w: 0,
    }
}

fn effects(sky: SceneSky, sun: SceneSun, ambient: SceneAmbient) -> SceneEffects {
    SceneEffects::new(SceneEffectsParts {
        water: None,
        sky,
        sun,
        ambient,
        shadows: SceneShadowCascades::new(vec![10_000, 40_000], 50_000)
            .expect("two ascending cascades are valid"),
        ssao: SceneSsao {
            radius_millimeters: 500,
            intensity_micro: 500_000,
            horizon_angle_millidegrees: 30_000,
        },
        camera: SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        grading: SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: Vec::new(),
        anti_alias: SceneAntiAlias::Disabled,
    })
    .expect("the fixture effects are valid")
}

fn procedural(zenith: SceneColor, horizon: SceneColor) -> SceneSky {
    SceneSky::ProceduralHemisphere { zenith, horizon }
}

fn grey(level: u8) -> SceneColor {
    SceneColor {
        red: level,
        green: level,
        blue: level,
    }
}

fn view() -> SceneView {
    SceneView::new(
        CameraPose {
            eye_millimeters: [1_000, 2_000, 3_000],
            target_millimeters: [1_000, 12_000, 3_000],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 1_000_000,
        },
        Viewport {
            width: 1_920,
            height: 1_080,
        },
    )
    .expect("the fixture pose is well formed")
}

fn uniform(sky: SceneSky, sun: SceneSun, ambient: SceneAmbient) -> FrameUniform {
    FrameUniform::new(&view(), &effects(sky, sun, ambient)).expect("the fixture frame builds")
}

fn lit() -> FrameUniform {
    uniform(
        procedural(grey(255), grey(255)),
        SceneSun {
            direction: east(),
            irradiance_micro: 1_000_000,
        },
        SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
    )
}

#[test]
fn the_block_matches_the_size_the_shader_declares() {
    // A mismatch here is not a compile error in either language; it silently
    // shifts every field after the first disagreement.
    assert_eq!(FRAME_UNIFORM_BYTES, 160);
    assert_eq!(lit().to_bytes().len(), 160);
}

#[test]
fn the_matrix_occupies_the_first_sixty_four_bytes_in_column_major_order() {
    let frame = lit();
    let bytes = frame.to_bytes();
    let expected = frame.view_projection.to_cols_array();
    for (index, value) in expected.iter().enumerate() {
        assert!(
            (read_f32(&bytes, index * 4) - value).abs() < f32::EPSILON,
            "matrix element {index} must round-trip"
        );
    }
}

#[test]
fn every_vector_lane_is_padded_to_sixteen_bytes() {
    // WGSL aligns vec4 members to sixteen bytes. Writing three floats per
    // vector without the fourth would pack the block tightly and misalign
    // every subsequent field.
    let bytes = lit().to_bytes();
    for offset in [76, 92, 108, 124, 140, 156] {
        assert!(
            read_f32(&bytes, offset).abs() < f32::EPSILON,
            "the padding lane at {offset} must be written, not left to chance"
        );
    }
}

#[test]
fn exposure_reaches_the_shader_in_its_own_lane() {
    // The grading block has always been decoded and has never reached a
    // shader, which is why the frame rendered about 2.2 stops under the
    // reference. The failure mode was silence: a correct value sitting in a
    // struct nothing read. Asserting the byte lands at the offset the shader
    // reads is what makes the plumbing observable rather than assumed.
    let bytes = lit().to_bytes();
    assert!(
        (read_f32(&bytes, 144) - 1.0).abs() < f32::EPSILON,
        "the default grading exposure of 1.0 must occupy the first grading lane"
    );
}

#[test]
fn the_reserved_grading_terms_are_not_given_stand_in_values() {
    // Contrast and saturation are decoded but unplumbed. Writing a plausible
    // default for them here would make an unimplemented term look authored,
    // and the shader would then be applying a value nobody chose.
    let bytes = lit().to_bytes();
    for offset in [148, 152] {
        assert!(
            read_f32(&bytes, offset).abs() < f32::EPSILON,
            "the reserved grading term at {offset} must stay zero until it is plumbed"
        );
    }
}

#[test]
fn the_sun_direction_stays_in_scene_space() {
    // The shader shades in Z-up, which is why its hemisphere term reads
    // normal.z. A sun converted to Y-up here would light the world from a
    // quarter-turn away while leaving geometry correctly placed.
    let frame = lit();
    assert!(
        (frame.sun_direction - Vec3::X).length() < 1.0e-6,
        "an eastward sun must stay eastward, got {:?}",
        frame.sun_direction
    );
    let bytes = frame.to_bytes();
    assert!((read_f32(&bytes, 80) - 1.0).abs() < 1.0e-6);
    assert!(read_f32(&bytes, 84).abs() < 1.0e-6);
    assert!(read_f32(&bytes, 88).abs() < 1.0e-6);
}

#[test]
fn the_camera_position_is_the_scene_space_eye_in_metres() {
    let frame = lit();
    assert!((frame.camera_position - Vec3::new(1.0, 2.0, 3.0)).length() < 1.0e-6);
}

#[test]
fn an_identity_rotation_is_reported_as_an_unauthored_sun() {
    // Every SceneSun in the tree is currently SceneRotation::IDENTITY, and no
    // authored contract supplies one at all. Reading that as a direction gives
    // the zero vector, which would light nothing while looking like a shading
    // bug. Saying so is more useful than rendering a black world.
    let error = FrameUniform::new(
        &view(),
        &effects(
            procedural(grey(255), grey(255)),
            SceneSun {
                direction: SceneRotation::IDENTITY,
                irradiance_micro: 1_000_000,
            },
            SceneAmbient {
                sky_irradiance_micro: 0,
                ground_irradiance_micro: 0,
            },
        ),
    )
    .expect_err("an identity sun direction is reported");
    assert_eq!(error, FrameError::SunDirectionUnset { x: 0, y: 0, z: 0 });
}

#[test]
fn a_genuine_rotation_quaternion_is_rejected_rather_than_silently_reinterpreted() {
    // SceneSun::direction is typed as a rotation but documented as a
    // direction. Requiring the vector part to be unit length settles the
    // ambiguity in one check: a real quaternion with a non-zero real part has
    // a shorter vector part and cannot be mistaken for a direction.
    let half = ROTATION_MICRO_SCALE / 2;
    let error = FrameUniform::new(
        &view(),
        &effects(
            procedural(grey(255), grey(255)),
            SceneSun {
                direction: SceneRotation {
                    x: 0,
                    y: 0,
                    z: half,
                    w: 866_025,
                },
                irradiance_micro: 1_000_000,
            },
            SceneAmbient {
                sky_irradiance_micro: 0,
                ground_irradiance_micro: 0,
            },
        ),
    )
    .expect_err("a rotation quaternion is not a direction");
    assert!(matches!(error, FrameError::SunDirectionUnset { .. }));
}

#[test]
fn sun_irradiance_scales_the_emitted_radiance() {
    let frame = uniform(
        procedural(grey(255), grey(255)),
        SceneSun {
            direction: east(),
            irradiance_micro: 250_000,
        },
        SceneAmbient {
            sky_irradiance_micro: 0,
            ground_irradiance_micro: 0,
        },
    );
    assert!((frame.sun_irradiance - Vec3::splat(0.25)).length() < 1.0e-6);
}

#[test]
fn sky_colour_is_decoded_with_the_piecewise_srgb_curve() {
    // A 2.2-power approximation differs by several percent at low levels,
    // which is exactly where ambient is the only light and the error shows.
    let frame = uniform(
        procedural(grey(10), grey(10)),
        SceneSun {
            direction: east(),
            irradiance_micro: 0,
        },
        SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
    );
    let expected = (10.0 / 255.0) / 12.92;
    assert!(
        (frame.ambient_sky.x - expected).abs() < 1.0e-6,
        "the linear segment must be used below the threshold, got {}",
        frame.ambient_sky.x
    );
    let approximation = (10.0f32 / 255.0).powf(2.2);
    assert!(
        (frame.ambient_sky.x - approximation).abs() > 1.0e-4,
        "the piecewise curve must be distinguishable from a 2.2 power"
    );
}

#[test]
fn the_zenith_lights_the_upper_hemisphere_and_the_horizon_the_lower() {
    // Light arriving from below has bounced off the ground, and the horizon
    // colour is the closest thing the procedural sky records to it. Swapping
    // the two inverts the vertical shading gradient across the whole world.
    let frame = uniform(
        procedural(grey(255), grey(0)),
        SceneSun {
            direction: east(),
            irradiance_micro: 0,
        },
        SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 1_000_000,
        },
    );
    assert!(frame.ambient_sky.x > 0.9, "the zenith drives the sky term");
    assert!(
        frame.ambient_ground.x < 1.0e-6,
        "the horizon drives the ground term"
    );
}

#[test]
fn ambient_irradiance_scales_each_hemisphere_independently() {
    let frame = uniform(
        procedural(grey(255), grey(255)),
        SceneSun {
            direction: east(),
            irradiance_micro: 0,
        },
        SceneAmbient {
            sky_irradiance_micro: 1_000_000,
            ground_irradiance_micro: 250_000,
        },
    );
    assert!((frame.ambient_sky.x - 1.0).abs() < 1.0e-6);
    assert!((frame.ambient_ground.x - 0.25).abs() < 1.0e-6);
}

#[test]
fn a_dome_sky_cannot_supply_hemispheric_ambient() {
    // Deriving ambient from a dome would mean prefiltering it, which this
    // renderer does not do. Substituting a neutral grey would be a silent
    // invention with no basis in the authored data.
    let error = FrameUniform::new(
        &view(),
        &effects(
            SceneSky::DomeTexture {
                texture: worldrunner_render_plan::TextureHandle::from_raw(0),
            },
            SceneSun {
                direction: east(),
                irradiance_micro: 1_000_000,
            },
            SceneAmbient {
                sky_irradiance_micro: 1_000_000,
                ground_irradiance_micro: 1_000_000,
            },
        ),
    )
    .expect_err("a dome sky is reported");
    assert_eq!(error, FrameError::AmbientFromDomeTexture);
}
