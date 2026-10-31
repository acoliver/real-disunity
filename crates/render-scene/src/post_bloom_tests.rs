use crate::{compositor::FrameRenderer, device::GpuContext};
use worldrunner_render_plan as s;
use worldrunner_render_post as p;

pub(crate) fn bloom() -> s::SceneBloom {
    s::SceneBloom {
        threshold_micro: 0,
        soft_knee_micro: 0,
        intensity_micro: 750_000,
        levels: 2,
        recipe: Some(s::SceneBloomRecipe {
            extraction: s::SceneBrightExtraction::ToneCurve(s::SceneToneCurveBright {
                curve: s::SceneBrightCurve::Reinhard,
                maximum_micro: 1_000_000,
                luminance_micro: [250_000, 500_000, 250_000],
                exposure_scale_micro: 1_000_000,
                contrast_micro: 1_000_000,
                epsilon_micro: 1_000,
            }),
            reduction: s::SceneBloomReduction::QuarterTexelAfterAverage,
            kernel: s::SceneGaussianKernelRule::AxisClampedPairs,
            selection: s::SceneBloomLevelSelection {
                first_blurred: 1,
                output: 1,
            },
            tint_pass: s::SceneBloomTintPass::Horizontal,
            terminal: s::SceneBloomTerminal::Repeat,
            levels: [s::SceneGaussianLevel {
                radius: 3,
                tint_micro: [500_000; 3],
                weight_micro: 1_000_000,
            }; 8],
        }),
    }
}

fn direct() -> p::BloomPyramid {
    p::BloomPyramid {
        bloom: p::Bloom {
            threshold: 0.0,
            knee: 0.0,
            intensity: 0.75,
            levels: 2,
            policies: p::BloomPolicies {
                extraction: p::BrightExtraction::ToneCurve(p::ToneCurveBright {
                    curve: p::BrightCurve::Reinhard,
                    maximum: 1.0,
                    luminance: [0.25, 0.5, 0.25],
                    exposure_scale: 1.0,
                    contrast: 1.0,
                    epsilon: 0.001,
                }),
                reduction: p::BloomReduction::QuarterTexelAfterAverage,
                kernel: p::GaussianKernelRule::AxisClampedPairs,
                selection: p::BloomLevelSelection {
                    first_blurred: 1,
                    output: 1,
                },
                tint_pass: p::BloomTintPass::Horizontal,
                terminal: p::BloomTerminal::Repeat,
            },
        },
        levels: [p::GaussianLevel {
            radius: 3,
            tint: [0.5; 3],
            weight: 1.0,
        }; 8],
    }
}

pub(crate) fn effects(bloom: s::SceneBloom) -> s::SceneEffects {
    s::SceneEffects::new(s::SceneEffectsParts {
        water: None,
        sky: s::SceneSky::ProceduralHemisphere {
            zenith: s::SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
            horizon: s::SceneColor {
                red: 255,
                green: 255,
                blue: 255,
            },
        },
        sun: s::SceneSun {
            direction: s::SceneDirection::new([0, 0, 1_000_000]).unwrap(),
            irradiance_micro: 1_000_000,
        },
        ambient: s::SceneAmbient {
            sky_irradiance_micro: 0,
            ground_irradiance_micro: 0,
        },
        shadows: s::SceneShadowCascades::new(vec![4_000], 0).unwrap(),
        ssao: None,
        camera: s::SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 20_000,
        },
        grading: s::SceneColorGrading {
            exposure_micro: 1_000_000,
            contrast_micro: 1_000_000,
            saturation_micro: 1_000_000,
        },
        post: vec![],
        anti_alias: s::SceneAntiAlias::Disabled,
    })
    .unwrap()
    .with_hdr_post(s::SceneHdrPost {
        tone_map: None,
        grading: None,
        adaptive_exposure: None,
        bloom: Some(bloom),
        fxaa_subpixel_micro: None,
        fxaa_quality: None,
    })
    .unwrap()
}

#[test]
fn scene_recipe_translates_to_direct_pyramid() {
    let mut authored = bloom();
    authored.recipe.as_mut().unwrap().levels[2] = s::SceneGaussianLevel {
        radius: 64,
        tint_micro: [250_000, 1_000_000, 64_000_000],
        weight_micro: 1_500_000,
    };
    let mut expected = direct();
    expected.levels[2] = p::GaussianLevel {
        radius: 64,
        tint: [0.25, 1.0, 64.0],
        weight: 1.5,
    };
    let actual = super::post_bloom::pyramid(authored);
    assert_eq!(actual.bloom, expected.bloom);
    assert_eq!(actual.levels, expected.levels);
    let settings = crate::post_settings(&effects(authored)).unwrap();
    assert_eq!(settings.bloom, Some(expected.bloom));
    assert_eq!(
        settings.bloom_kernel,
        p::BloomKernel::SeparablePyramid(expected.levels)
    );
}

#[test]
fn alternate_choices_and_full_half_float_scalars_translate() {
    let mut b = bloom();
    let r = b.recipe.as_mut().unwrap();
    r.extraction = s::SceneBrightExtraction::SoftKnee;
    r.reduction = s::SceneBloomReduction::BoxPerTexel;
    r.kernel = s::SceneGaussianKernelRule::RadiusThird;
    r.selection = s::SceneBloomLevelSelection {
        first_blurred: 0,
        output: 0,
    };
    r.tint_pass = s::SceneBloomTintPass::Vertical;
    r.terminal = s::SceneBloomTerminal::Reject;
    let out = crate::post_settings(&effects(b)).unwrap();
    assert_eq!(out.bloom.unwrap().policies, p::BloomPolicies::default());
    b.recipe.as_mut().unwrap().extraction =
        s::SceneBrightExtraction::ToneCurve(s::SceneToneCurveBright {
            curve: s::SceneBrightCurve::AcesFilmic,
            maximum_micro: 65_504_000_000,
            luminance_micro: [0, 1_000_000, 0],
            exposure_scale_micro: 65_504_000_000,
            contrast_micro: 65_504_000_000,
            epsilon_micro: 1,
        });
    let out = crate::post_settings(&effects(b)).unwrap();
    assert_eq!(
        out.bloom.unwrap().policies.extraction,
        p::BrightExtraction::ToneCurve(p::ToneCurveBright {
            curve: p::BrightCurve::AcesFilmic,
            maximum: 65_504.0,
            luminance: [0.0, 1.0, 0.0],
            exposure_scale: 65_504.0,
            contrast: 65_504.0,
            epsilon: 0.000_001,
        })
    );
}

fn capture(
    context: &GpuContext,
    source: &wgpu::TextureView,
    encode: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView),
) -> Vec<u8> {
    let output = p::color_texture(
        context.device(),
        "recipe output",
        [2, 2],
        wgpu::TextureFormat::Rgba8UnormSrgb,
    );
    let mut encoder = context.submission().begin(context.device()).unwrap();
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: source,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.5,
                        g: 0.5,
                        b: 0.5,
                        a: 1.0,
                    }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
    }
    encode(
        &mut encoder,
        &output.create_view(&wgpu::TextureViewDescriptor::default()),
    );
    encoder.finish().submit().unwrap();
    p::Readback::new(
        context.device(),
        p::FrameConfig::new([2, 2], output.format(), 1),
        4096,
    )
    .unwrap()
    .read(context.device(), context.submission(), &output)
    .unwrap()
}

fn scene_pixels(context: &GpuContext, effects: &s::SceneEffects) -> Vec<u8> {
    let frame = FrameRenderer::hdr(
        context,
        [2, 2],
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1,
        p::ToneMap::Reinhard,
        Some(effects),
    )
    .unwrap();
    capture(context, frame.targets().resolved(), |encoder, output| {
        frame.encode_output(encoder, output).unwrap();
    })
}

fn direct_pixels(context: &GpuContext, settings: &p::HdrPostSettings) -> Vec<u8> {
    let factory = p::ToneMapOutput::new(
        wgpu::TextureFormat::Rgba8UnormSrgb,
        1.0,
        p::ToneMap::Reinhard,
    )
    .unwrap()
    .with_effects(context.device(), *settings)
    .unwrap();
    let chain = p::PostChain::new(
        context.device(),
        p::FrameConfig::new([2, 2], wgpu::TextureFormat::Rgba16Float, 1),
        p::Resources::default(),
        factory,
    )
    .unwrap();
    capture(context, chain.targets().resolved(), |encoder, output| {
        chain
            .factory()
            .prepare_effects(encoder, 0, 0.0, true)
            .unwrap();
        chain.encode(encoder, output);
    })
}

/// Asserts every pixel is the display value of uniform half radiance plus
/// `Reinhard(0.5) * tint 0.5 * intensity`, within the rounding bound and the half stores.
///
/// The pipeline stores to `Rgba16Float` at most once per pass: extraction, downsample, two blur
/// passes and the composite per level for two levels is under eight stores. Seven of them hold
/// the extracted bloom value Reinhard(0.5) = 1/3 (the blurs are convex, so their errors add),
/// each off by at most 2^-10 of it, and the bloom scale `0.5 * intensity` applies once afterwards.
/// The composite store errs by 2^-10 of the sum. Reinhard divides a sum error by `(1 + sum)^2`,
/// and the steepest slope of the encoding below `linear` is found as a difference. At intensity
/// 0.75 (sum 0.625) the bound is about 0.74 codes; at intensity 2 (sum 0.833) it is about 0.77
/// while a 10% intensity error moves the code by about 1.9.
fn assert_uniform_bloom(pixels: &[u8], intensity: f64) {
    let sum = 0.5 + 0.5 * intensity / 3.0;
    let linear = sum / (1.0 + sum);
    let encoded_at = worldrunner_render_gpu::tolerance_test_support::encoded_steps;
    let sum_error = 0.5 * intensity * 7.0 / 3.0 / 1024.0 + sum / 1024.0;
    let linear_error = sum_error / ((1.0 + sum) * (1.0 + sum));
    let stores = encoded_at(linear, true) - encoded_at(linear - linear_error, true);
    for pixel in pixels.chunks_exact(4) {
        for &code in &pixel[..3] {
            worldrunner_render_gpu::tolerance_test_support::assert_code_within(
                code,
                encoded_at(linear, true),
                stores,
                &format!("uniform bloom recipe, intensity {intensity}"),
            );
        }
        assert_eq!(pixel[3], 255, "alpha");
    }
}

#[test]
fn gpu_scene_recipe_matches_direct_and_omission_preserves_tent_bytes() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let pyramid = direct();
    let actual = scene_pixels(&context, &effects(bloom()));
    let expected = direct_pixels(
        &context,
        &p::HdrPostSettings {
            bloom: Some(pyramid.bloom),
            bloom_kernel: p::BloomKernel::SeparablePyramid(pyramid.levels),
            ..Default::default()
        },
    );
    // The scene recipe translates to the same post settings as the direct ones, so both renders run
    // the same shader code on the same device and stay byte identical.
    assert_eq!(actual, expected);
    assert_uniform_bloom(&actual, 0.75);
    // At 0.75 the bloom term is a fifth of the sum, so a 10% intensity error moves the code by
    // less than the rounding bound; intensity 2 makes the same error exceed the derived bound.
    let strong = s::SceneBloom {
        intensity_micro: 2_000_000,
        ..bloom()
    };
    assert_uniform_bloom(&scene_pixels(&context, &effects(strong)), 2.0);
    let omitted = s::SceneBloom {
        recipe: None,
        ..bloom()
    };
    let legacy = p::Bloom {
        policies: p::BloomPolicies::default(),
        ..pyramid.bloom
    };
    let translated = crate::post_settings(&effects(omitted)).unwrap();
    assert_eq!(translated.bloom_kernel, p::BloomKernel::Tent);
    assert_eq!(translated.bloom, Some(legacy));
    assert_eq!(
        scene_pixels(&context, &effects(omitted)),
        direct_pixels(
            &context,
            &p::HdrPostSettings {
                bloom: Some(legacy),
                ..Default::default()
            }
        )
    );
    context.device().poll(wgpu::PollType::Poll).unwrap();
}
