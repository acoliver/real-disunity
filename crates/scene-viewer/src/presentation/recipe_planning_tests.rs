use crate::{CaptureJitter, CaptureOutput, PresentationCapture, RetainedCaptureRecipe};
use worldrunner_render_gpu::configuration::AllocationBudgetPolicy;

#[test]
fn endpoint_seed_storage_reaches_presentation_plan_and_derived_budget() {
    use SceneWaterSeedCoordinates::{EndpointClamped, Repeat};
    use SceneWaterSeedSampling::{Fragment, Vertex};
    use worldrunner_render_plan::{
        SceneWaterFlowCoordinates, SceneWaterSeedCoordinates, SceneWaterSeedSampling, TextureHandle,
    };
    let scene = crate::tests::scene(true);
    let effects = scene.effects().unwrap();
    let mut body = effects.water().unwrap().clone();
    body.material.flow = Some(TextureHandle::from_raw(0));
    let limits = wgpu::Limits::default();
    let options = crate::ViewerOptions {
        frame_profile: crate::FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        ..crate::tests::options()
    };
    let policy = AllocationBudgetPolicy::Derived {
        headroom_bytes: 0,
        granularity_bytes: 1,
        cap_bytes: None,
    };
    let mut totals = Vec::new();
    for (mode, stage) in [
        (Repeat, Fragment),
        (EndpointClamped, Fragment),
        (Repeat, Vertex),
        (EndpointClamped, Vertex),
    ] {
        body.material.flow_seed_sampling = stage;
        body.material.flow_coordinates = Some(SceneWaterFlowCoordinates {
            origin_millimeters: [0; 2],
            tile_millimeters: [1000; 2],
            mode,
        });
        let effects = effects
            .clone()
            .with_water_bodies(vec![body.clone()])
            .unwrap();
        let plan = crate::plan_presentation(
            [32, 24],
            &options,
            Some(&effects),
            &limits,
            wgpu::Features::empty(),
            crate::PresentationPlanning {
                capture: PresentationCapture::Ordinary,
                retained_effect_history: false,
            },
        )
        .unwrap();
        let required = plan.validate(&limits).unwrap();
        assert_eq!(policy.resolve::<()>(required).unwrap().0, required);
        totals.push(required);
    }
    assert_eq!(totals[1], totals[0] + 48);
    assert_eq!(&totals[..2], &totals[2..]);
}

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
