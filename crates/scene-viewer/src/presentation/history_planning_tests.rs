use super::*;
use crate::{FrameProfile, PresentationBudgetPolicy, PresentationCapture, PresentationPlanning};
use worldrunner_render_gpu::configuration::{AllocationSet, ConfigurationError};
use worldrunner_render_scene::device::GpuContext;

const SIZE: Viewport = Viewport {
    width: 8,
    height: 8,
};
const NEXT: Viewport = Viewport {
    width: 16,
    height: 8,
};

fn options() -> crate::ViewerOptions {
    crate::ViewerOptions {
        frame_profile: FrameProfile::Hdr {
            samples: 1,
            tone_map: crate::ToneMap::Reinhard,
        },
        presentation_budget: PresentationBudgetPolicy::Derived {
            headroom_bytes: 0,
            granularity_bytes: 1,
            cap_bytes: None,
        },
        ..crate::tests::options()
    }
}

fn rcas() -> crate::NativeRcasSettings {
    crate::NativeRcasSettings {
        stops: 0.5,
        domain: crate::RcasDomain::EncodedOutput,
    }
}

fn presentation(context: &GpuContext, options: &crate::ViewerOptions) -> Presentation {
    Presentation::with_settings(
        context,
        COLOR,
        [SIZE, SIZE],
        options.frame_profile,
        None,
        &PresentationPolicy {
            budget: options.presentation_budget,
            capture_pool: crate::CapturePoolPolicy::default(),
            recipes: true,
        },
    )
    .unwrap()
}

fn planned(
    context: &GpuContext,
    options: &crate::ViewerOptions,
    size: Viewport,
    capture: PresentationCapture<'_>,
    retained_effect_history: bool,
) -> AllocationSet {
    crate::plan_presentation(
        [size.width, size.height],
        options,
        None,
        &context.device().limits(),
        context.device().features(),
        PresentationPlanning {
            capture,
            retained_effect_history,
        },
    )
    .unwrap()
}

fn peak(context: &GpuContext, current: &AllocationSet, next: &AllocationSet) -> u64 {
    let mut union = current.clone();
    union.extend(next.clone());
    union.validate(&context.device().limits()).unwrap()
}

fn fresh_peak(context: &GpuContext, options: &crate::ViewerOptions, next: Viewport) -> u64 {
    let current = planned(context, options, SIZE, PresentationCapture::Retained, false);
    let next = planned(context, options, next, PresentationCapture::Retained, false);
    peak(context, &current, &next)
}

fn collect(context: &GpuContext, presentation: &mut Presentation) {
    context
        .device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    presentation.scheduler.collect();
    assert_eq!(
        context.submission().submitted_serial(),
        context.submission().completed_serial()
    );
}

fn replace_profile(
    context: &GpuContext,
    presentation: &mut Presentation,
    options: &crate::ViewerOptions,
    current: &AllocationSet,
) -> AllocationSet {
    let next = planned(
        context,
        options,
        SIZE,
        PresentationCapture::Retained,
        presentation.scheduler.has_effect_history(),
    );
    let (mut settings, _) = setup::configuration(
        context.device().features(),
        COLOR,
        SIZE,
        options.frame_profile,
        None,
    )
    .unwrap();
    settings.additional = planning::additional(SIZE, Some(256 * 1024 * 1024));
    presentation
        .scheduler
        .reconfigure(context, settings)
        .unwrap();
    assert_eq!(
        peak(context, current, &next),
        presentation.scheduler.budget().0
    );
    next
}

#[test]
fn post_enable_disable_and_collected_replacement_match_live_budget() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut options = options();
    let mut presentation = presentation(&context, &options);
    let initial = planned(
        &context,
        &options,
        SIZE,
        PresentationCapture::Retained,
        false,
    );
    assert_eq!(
        initial.validate(&context.device().limits()).unwrap(),
        presentation.scheduler.budget().0
    );
    context
        .submission()
        .begin(context.device())
        .unwrap()
        .finish()
        .submit()
        .unwrap();
    assert!(context.submission().submitted_serial() > context.submission().completed_serial());
    options.frame_profile = FrameProfile::HdrNativeRcas {
        samples: 1,
        tone_map: crate::ToneMap::Reinhard,
        rcas: rcas(),
    };
    let enabled = replace_profile(&context, &mut presentation, &options, &initial);
    assert!(presentation.scheduler.has_effect_history());
    assert!(context.submission().submitted_serial() > context.submission().completed_serial());
    let mut live = initial;
    live.extend(enabled);
    options.frame_profile = self::options().frame_profile;
    let disabled = replace_profile(&context, &mut presentation, &options, &live);
    assert!(!presentation.scheduler.configuration().post.enabled());
    collect(&context, &mut presentation);
    assert_later_replacement(&context, &mut presentation, &options, &disabled);
}

fn assert_later_replacement(
    context: &GpuContext,
    presentation: &mut Presentation,
    options: &crate::ViewerOptions,
    current: &AllocationSet,
) {
    let next = planned(context, options, NEXT, PresentationCapture::Retained, true);
    let fresh_current = planned(context, options, SIZE, PresentationCapture::Retained, false);
    let fresh_next = planned(context, options, NEXT, PresentationCapture::Retained, false);
    let required = peak(context, current, &next);
    assert_eq!(required, peak(context, &fresh_current, &fresh_next) + 208);
    presentation.resize(context, NEXT, NEXT).unwrap();
    assert_eq!(required, presentation.scheduler.budget().0);
}

fn recipe() -> crate::RetainedCaptureRecipe {
    crate::RetainedCaptureRecipe::new(
        1,
        2,
        crate::CaptureJitter::None,
        false,
        vec![crate::CaptureOutput::Srgb8],
        1024 * 1024,
    )
    .unwrap()
    .with_native_rcas(rcas())
    .unwrap()
}

fn restore_recipe(
    context: &GpuContext,
    presentation: &mut Presentation,
    options: &crate::ViewerOptions,
) -> AllocationSet {
    let initial = planned(context, options, SIZE, PresentationCapture::Retained, false);
    let recipe = recipe();
    let recipe_plan = planned(
        context,
        options,
        SIZE,
        PresentationCapture::Recipe(&recipe),
        false,
    );
    presentation
        .resize_recipe(context, SIZE, recipe.native_rcas())
        .unwrap();
    assert_eq!(
        peak(context, &initial, &recipe_plan),
        presentation.scheduler.budget().0
    );
    assert!(presentation.scheduler.has_effect_history());
    collect(context, presentation);
    let restored = planned(context, options, SIZE, PresentationCapture::Retained, true);
    presentation.resize(context, SIZE, SIZE).unwrap();
    assert_eq!(
        peak(context, &recipe_plan, &restored),
        presentation.scheduler.budget().0
    );
    assert!(!presentation.scheduler.configuration().post.enabled());
    collect(context, presentation);
    restored
}

#[test]
fn restored_recipe_history_matches_planned_replacement_peak() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let options = options();
    let mut presentation = presentation(&context, &options);
    let restored = restore_recipe(&context, &mut presentation, &options);
    assert_later_replacement(&context, &mut presentation, &options, &restored);
}

#[test]
fn retained_history_cap_rejection_matches_planned_union_without_publication() {
    let context = pollster::block_on(GpuContext::request()).unwrap();
    let mut options = options();
    let large = Viewport {
        width: 32,
        height: 8,
    };
    let cap = fresh_peak(&context, &options, large);
    options.presentation_budget = PresentationBudgetPolicy::Derived {
        headroom_bytes: 0,
        granularity_bytes: 1,
        cap_bytes: Some(cap),
    };
    let mut presentation = presentation(&context, &options);
    let restored = restore_recipe(&context, &mut presentation, &options);
    let next = planned(
        &context,
        &options,
        large,
        PresentationCapture::Retained,
        true,
    );
    let required = peak(&context, &restored, &next);
    assert_eq!(required, cap + 208);
    let before = (
        presentation.scheduler.generation(),
        presentation.scheduler.budget(),
    );
    let texture = presentation.scheduler.targets().texture().clone();
    assert!(matches!(presentation.resize(&context, large, large),
        Err(ViewerError::Configuration(ConfigurationError::Budget { required: actual, budget }))
        if actual == required && budget == cap));
    assert_eq!(
        (
            presentation.scheduler.generation(),
            presentation.scheduler.budget()
        ),
        before
    );
    assert_eq!(presentation.scheduler.targets().texture(), &texture);
    assert!(presentation.scheduler.has_effect_history());
}
