//! Scene quantities translated into the existing separable bloom builder.
use super::post::micro;
use worldrunner_render_plan as scene;
use worldrunner_render_post as post;

pub(crate) fn pyramid(b: scene::SceneBloom) -> post::BloomPyramid {
    let mut out = post::BloomPyramid {
        bloom: post::Bloom {
            policies: post::BloomPolicies::default(),
            threshold: micro(b.threshold_micro),
            knee: micro(b.soft_knee_micro),
            intensity: micro(b.intensity_micro),
            levels: b.levels,
        },
        levels: [post::GaussianLevel {
            radius: 0,
            tint: [1.0; 3],
            weight: 1.0,
        }; 8],
    };
    if let Some(r) = b.recipe {
        out.bloom.policies = policies(r);
        out.levels = r.levels.map(|l| post::GaussianLevel {
            radius: l.radius,
            tint: l.tint_micro.map(micro),
            weight: micro(l.weight_micro),
        });
    }
    out
}

fn policies(r: scene::SceneBloomRecipe) -> post::BloomPolicies {
    post::BloomPolicies {
        extraction: match r.extraction {
            scene::SceneBrightExtraction::SoftKnee => post::BrightExtraction::SoftKnee,
            scene::SceneBrightExtraction::ToneCurve(c) => {
                post::BrightExtraction::ToneCurve(post::ToneCurveBright {
                    curve: match c.curve {
                        scene::SceneBrightCurve::AcesFilmic => post::BrightCurve::AcesFilmic,
                        scene::SceneBrightCurve::Reinhard => post::BrightCurve::Reinhard,
                    },
                    maximum: wide_micro(c.maximum_micro),
                    luminance: c.luminance_micro.map(micro),
                    exposure_scale: wide_micro(c.exposure_scale_micro),
                    contrast: wide_micro(c.contrast_micro),
                    epsilon: wide_micro(c.epsilon_micro),
                })
            }
        },
        reduction: match r.reduction {
            scene::SceneBloomReduction::BoxPerTexel => post::BloomReduction::BoxPerTexel,
            scene::SceneBloomReduction::QuarterTexelAfterAverage => {
                post::BloomReduction::QuarterTexelAfterAverage
            }
        },
        kernel: match r.kernel {
            scene::SceneGaussianKernelRule::RadiusThird => post::GaussianKernelRule::RadiusThird,
            scene::SceneGaussianKernelRule::AxisClampedPairs => {
                post::GaussianKernelRule::AxisClampedPairs
            }
        },
        selection: post::BloomLevelSelection {
            first_blurred: r.selection.first_blurred,
            output: r.selection.output,
        },
        tint_pass: match r.tint_pass {
            scene::SceneBloomTintPass::Vertical => post::BloomTintPass::Vertical,
            scene::SceneBloomTintPass::Horizontal => post::BloomTintPass::Horizontal,
        },
        terminal: match r.terminal {
            scene::SceneBloomTerminal::Reject => post::BloomTerminal::Reject,
            scene::SceneBloomTerminal::Repeat => post::BloomTerminal::Repeat,
        },
    }
}

fn wide_micro(value: u64) -> f32 {
    f32::from(u16::try_from(value / 1_000_000).expect("validated bloom scalar"))
        + micro(u32::try_from(value % 1_000_000).unwrap())
}
