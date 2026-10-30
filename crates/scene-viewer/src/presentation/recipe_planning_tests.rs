use crate::{CaptureJitter, CaptureOutput, PresentationCapture, RetainedCaptureRecipe};
use worldrunner_render_gpu::configuration::AllocationBudgetPolicy;

#[test]
fn water_recipe_plans_match_host_descriptors_at_both_backing_scales() {
    let limits = wgpu::Limits::default();
    let scene = crate::tests::scene(true);
    for samples in [1, 4] {
        let options = crate::ViewerOptions {
            frame_profile: crate::FrameProfile::Hdr {
                samples,
                tone_map: crate::ToneMap::Reinhard,
            },
            ..crate::tests::options()
        };
        for size in [[320, 240], [640, 480]] {
            for (supersample, temporal) in [(1, 1), (2, 1), (1, 4)] {
                let recipe = RetainedCaptureRecipe::new(
                    supersample,
                    temporal,
                    CaptureJitter::None,
                    false,
                    vec![CaptureOutput::Srgb8],
                    256 * 1024 * 1024,
                )
                .unwrap();
                verify_recipe_plan(size, &options, scene.effects(), &recipe, &limits);
            }
        }
    }
}

fn verify_recipe_plan(
    size: [u32; 2],
    options: &crate::ViewerOptions,
    effects: Option<&worldrunner_render_plan::SceneEffects>,
    recipe: &RetainedCaptureRecipe,
    limits: &wgpu::Limits,
) {
    let planned = crate::plan_presentation(
        size,
        options,
        effects,
        limits,
        wgpu::Features::empty(),
        crate::PresentationPlanning {
            capture: PresentationCapture::Recipe(recipe),
            retained_effect_history: false,
        },
    )
    .unwrap();
    let raster = recipe.render_size(size).unwrap();
    let viewport = worldrunner_render_scene::Viewport {
        width: raster[0],
        height: raster[1],
    };
    let (mut settings, _) = super::setup::configuration(
        wgpu::Features::empty(),
        super::COLOR,
        viewport,
        options.frame_profile,
        effects,
    )
    .unwrap();
    settings.additional = super::planning::additional(viewport, true);
    assert_eq!(
        planned.validate(limits),
        settings.allocations().validate(limits)
    );
    let mut peak = crate::plan_presentation(
        size,
        options,
        effects,
        limits,
        wgpu::Features::empty(),
        crate::PresentationPlanning {
            capture: PresentationCapture::Retained,
            retained_effect_history: false,
        },
    )
    .unwrap();
    let ordinary = peak.validate(limits).unwrap();
    peak.extend(planned);
    let required = peak.validate(limits).unwrap();
    assert!(required > ordinary);
    let policy = AllocationBudgetPolicy::Derived {
        headroom_bytes: 1024,
        granularity_bytes: 4096,
        cap_bytes: None,
    };
    assert!(policy.resolve::<()>(required).unwrap().0 >= required + 1024);
}
