use super::foliage::{geometry_sources, pass_depth};
use super::lod_material::{scene, selection_frame};
use super::*;
use crate::offscreen::OffscreenTarget;
use worldrunner_render_plan::SceneFadeBand;

fn depth_outputs(context: &GpuContext, gpu: &SceneGpu, pipelines: &ScenePipelines) -> Vec<Vec<u8>> {
    geometry_sources()
        .iter()
        .enumerate()
        .map(|(index, source)| {
            let entry = if index == 3 {
                "fs_depth_masked"
            } else {
                "fs_masked"
            };
            pass_depth(context, gpu, pipelines, source, entry, index != 3)
        })
        .collect()
}

#[test]
fn scaled_retained_lod_agrees_in_forward_metadata_prepass_and_shadow() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let source = scene(true);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, &pipelines).unwrap();
    let handle = gpu.instance_handles().next().unwrap();
    let object = gpu.instance(handle).unwrap().0;
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    gpu.set_frame(context.queue(), selection_frame(5_000));
    let default = target.render(&context, &gpu, &pipelines).unwrap();
    let generation = gpu.caster_generation();
    gpu.set_instance_object(
        handle,
        object.with_lod_distance_scale_millionths(500_000).unwrap(),
    )
    .unwrap();
    assert_ne!(gpu.caster_generation(), generation);
    gpu.flush_instances(context.device(), context.submission())
        .unwrap();
    let scaled = target.render(&context, &gpu, &pipelines).unwrap();
    assert_ne!(scaled, default);
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![0, 1]);
    let outputs = depth_outputs(&context, &gpu, &pipelines);
    assert!(
        outputs[0]
            .chunks_exact(4)
            .any(|p| p != 1.0_f32.to_le_bytes())
    );
    assert!(outputs.iter().all(|depth| *depth == outputs[0]));
    gpu.set_instance_object(handle, object).unwrap();
    gpu.flush_instances(context.device(), context.submission())
        .unwrap();
    assert_eq!(target.render(&context, &gpu, &pipelines).unwrap(), default);
}

#[test]
fn lod_scale_preserves_explicit_world_distance_fade_endpoints() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    for terminal in [false, true] {
        let band = SceneFadeBand::new(1_000, 2_000).unwrap();
        let band = if terminal {
            band.with_terminal_fade()
        } else {
            band
        };
        assert_world_distance_band(&context, &pipelines, band);
    }
}

fn assert_world_distance_band(
    context: &GpuContext,
    pipelines: &ScenePipelines,
    band: SceneFadeBand,
) {
    let source = scene(true);
    let mut gpu =
        SceneGpu::upload(context.device(), context.submission(), &source, pipelines).unwrap();
    let handle = gpu.instance_handles().next().unwrap();
    let object = gpu.instance(handle).unwrap().0.with_fade(band);
    for distance in [1_000, 1_500, 2_000] {
        gpu.set_frame(context.queue(), selection_frame(distance));
        let mut reference = None;
        for replacement in [
            object,
            object.with_lod_distance_scale_millionths(500_000).unwrap(),
            object
                .with_lod_distance_scale_millionths(2_000_000)
                .unwrap(),
        ] {
            gpu.set_instance_object(handle, replacement).unwrap();
            gpu.flush_instances(context.device(), context.submission())
                .unwrap();
            let output = pass_depth(
                context,
                &gpu,
                pipelines,
                crate::shader::ENVIRONMENT_WGSL,
                "fs_masked",
                true,
            );
            let covered = output
                .chunks_exact(4)
                .filter(|p| *p != 1.0_f32.to_le_bytes())
                .count();
            let visible = if band.terminal() {
                distance < 2_000
            } else {
                distance > 1_000
            };
            assert_eq!(covered > 0, visible);
            if let Some(reference) = &reference {
                assert_eq!(&output, reference, "distance={distance}, band={band:?}");
            }
            reference = Some(output);
        }
    }
}
