//! Deterministic sampling-cost regression plus informational ordinary-scene GPU times.
use super::*;
use worldrunner_render_post::{PassId, TimingMode, TimingSupport};

#[test]
fn ordinary_material_gpu_sampling_cost() {
    let base = context();
    let context = pollster::block_on(GpuContext::from_adapter_with_timing(
        base.adapter().clone(),
        wgpu::Features::empty(),
        TimingMode::Optional,
    ))
    .unwrap();
    let casters: Vec<_> = (0..1_024)
        .map(|i| [(i % 32) * 150 - 2_325, (i / 32) * 150 - 2_325, 1_000])
        .collect();
    for alpha in [None, Some([255, 0, 255, 0])] {
        let source = scene(&casters, 600_000, alpha);
        report_sampling_time(&context, &source);
    }
    crate::shader::impostor_cost_tests::assert_ordinary_material_cost();
}

fn report_sampling_time(context: &GpuContext, source: &RenderScene) {
    let mut renderer = ShadowedScene::new(context, source, settings()).unwrap();
    let support = renderer
        .scheduler
        .configure_timing(context.device(), context.queue(), TimingMode::Optional)
        .unwrap();
    if support != TimingSupport::Supported {
        println!("ordinary material GPU timestamps unavailable");
        return;
    }
    let view = view();
    let target = OffscreenTarget::new(context.device(), view.viewport()).unwrap();
    let mut opaque_ns = Vec::new();
    for frame in 0..40 {
        let timing = timing_probe::timed_frame(&mut renderer, context, &view, &target, frame);
        if frame >= 8 {
            opaque_ns.extend(timing.passes.iter().filter_map(|pass| {
                (pass.pass_id == PassId::Geometry(PassSlot::Opaque))
                    .then_some(pass.duration_ns)
                    .flatten()
            }));
        }
    }
    let alpha = source.resources().materials()[1].alpha_mode();
    if opaque_ns.is_empty() {
        println!("ordinary {alpha:?} GPU timestamps unavailable (no valid opaque pairs)");
    } else {
        opaque_ns.sort_by(f64::total_cmp);
        println!(
            "ordinary {alpha:?}: opaque GPU median {:.3} ms, {} valid samples; 1024 casters, 257x241",
            opaque_ns[opaque_ns.len() / 2] / 1_000_000.0,
            opaque_ns.len()
        );
    }
}
