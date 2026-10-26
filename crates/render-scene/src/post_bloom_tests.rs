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
    assert_eq!(actual, expected);
    // Uniform half radiance plus (Reinhard(0.5) * tint 0.5 * intensity 0.75).
    let linear: f64 = 0.625 / 1.625;
    let encoded = (1.055 * linear.powf(1.0 / 2.4) - 0.055) * 255.0;
    assert!((encoded - (encoded.floor() + 0.5)).abs() > 0.1);
    assert!(actual.chunks_exact(4).all(|p| p == [167, 167, 167, 255]));
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
