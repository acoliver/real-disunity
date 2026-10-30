use super::*;
use worldrunner_render_plan::SceneEnvironmentConsumers::{self, Both, Reflections, Water};

fn maps(context: &GpuContext, intensity: u32) -> crate::sky::preparation::PreparedEnvironmentMaps {
    let base = environment(context);
    let lut = worldrunner_render_plan::SceneTexture::Texels {
        format: worldrunner_render_plan::TexelFormat::Rg16Float,
        color_space: worldrunner_render_plan::TextureColorSpace::Linear,
        width: 1,
        height: 1,
        mips: 1,
        bytes: [0x3800u16, 0x3400]
            .into_iter()
            .flat_map(u16::to_le_bytes)
            .collect::<Vec<_>>()
            .into(),
    };
    let lut = crate::upload::upload_texture(
        context.device(),
        context.queue(),
        &crate::texture::prepare_texture(&lut).unwrap(),
    )
    .unwrap();
    crate::sky::preparation::PreparedEnvironmentMaps::from_uploaded(
        base.views().unwrap().0.texture(),
        &lut,
        base.convention(),
        intensity,
    )
    .unwrap()
}

struct Probe {
    color: Vec<f32>,
    opaque: Vec<f32>,
    factor: Vec<f32>,
    water: Vec<f32>,
    confidence: Vec<f32>,
    water_flag: Vec<f32>,
}
fn probe(
    context: &GpuContext,
    scene: &RenderScene,
    intensity: u32,
    consumers: SceneEnvironmentConsumers,
) -> Probe {
    let maps = maps(context, intensity);
    let renderer = render_selected_environment(context, scene, 1, &maps, &frame(scene), consumers);
    let ssr = renderer.ssr.as_ref().unwrap();
    Probe {
        color: channel(context, renderer.targets().scene().texture(), 0, 0),
        opaque: channel(context, ssr.textures()[2], 0, 0),
        factor: channel(context, ssr.textures()[3], 0, 0),
        water: channel(
            context,
            renderer.water_receivers().unwrap().2.texture(),
            0,
            0,
        ),
        confidence: channel(context, ssr.textures()[4], 0, 3),
        water_flag: channel(
            context,
            renderer.water_receivers().unwrap().0.texture(),
            0,
            3,
        ),
    }
}
fn assert_affine(zero: &[f32], unit: &[f32], double: &[f32]) {
    for ((zero, unit), double) in zero.iter().zip(unit).zip(double) {
        // Three binary16 stores, with values bounded by four in this fixture.
        let tolerance = 3.0 * 4.0 * 2.0_f32.powi(-10);
        assert!(
            (double - (2.0 * unit - zero)).abs() <= tolerance,
            "nonlinear lobe: {zero}, {unit}, {double}"
        );
    }
}

#[test]
fn uploaded_intensity_scales_selected_lobes_without_scaling_hit_factors() {
    let context = context();
    for consumers in [Water, Reflections, Both] {
        for missing in [true, false] {
            let scene = source(true, missing);
            let zero = probe(&context, &scene, 0, consumers);
            let unit = probe(&context, &scene, 1_000_000, consumers);
            let double = probe(&context, &scene, 2_000_000, consumers);
            assert_eq!(zero.factor, unit.factor);
            assert_eq!(unit.factor, double.factor);
            assert!(zero.opaque.iter().all(|value| *value == 0.0));
            assert!(zero.water.iter().all(|value| *value == 0.0));
            assert_eq!(unit.water.iter().any(|v| *v > 0.0), consumers.water());
            if !missing {
                assert_eq!(
                    unit.opaque.iter().any(|v| *v > 0.0),
                    consumers.reflections()
                );
                assert!(zero.confidence.iter().any(|v| *v > 0.0));
                if consumers.water() {
                    assert!(
                        zero.color
                            .iter()
                            .zip(&zero.water_flag)
                            .zip(&zero.confidence)
                            .any(|((color, water), confidence)| *water > 0.0
                                && *confidence > 0.0
                                && *color > 0.0),
                        "zero cube retains water scene-hit radiance"
                    );
                }
            }
            assert_affine(&zero.opaque, &unit.opaque, &double.opaque);
            assert_affine(&zero.water, &unit.water, &double.water);
            assert_affine(&zero.color, &unit.color, &double.color);
        }
    }
}

#[test]
fn forward_diffuse_and_direct_residual_are_independent_of_cube_scale() {
    let context = context();
    let scene = source(false, false);
    let effects = SceneEffects::new(effect_parts())
        .unwrap()
        .with_ssr(SceneSsr {
            max_steps: 1,
            origin_skip_pixels: 4096,
            ..request()
        })
        .unwrap();
    let scene = scene
        .with_objects_and_effects(scene.geometry().objects().to_vec(), Some(effects))
        .unwrap();
    let mut residuals = Vec::new();
    for intensity in [0, 1_000_000, 2_000_000] {
        let maps = maps(&context, intensity);
        let renderer =
            render_with_environment(&context, &scene, 1, &maps, &texel_center_frame(&scene));
        let color = channel(&context, renderer.targets().scene().texture(), 0, 0);
        let ssr = renderer.ssr.as_ref().unwrap();
        let environment = channel(&context, ssr.textures()[2], 0, 0);
        assert!(
            channel(&context, ssr.textures()[4], 0, 3)
                .iter()
                .all(|v| *v == 0.0)
        );
        residuals.push(
            color
                .iter()
                .zip(environment)
                .map(|(c, e)| c - e)
                .collect::<Vec<_>>(),
        );
    }
    for actual in &residuals[1..] {
        for (actual, expected) in actual.iter().zip(&residuals[0]) {
            assert!((actual - expected).abs() <= 2.0 * 2.0_f32.powi(-10));
        }
    }
}
