use super::lod_material::{scene, selection_frame};
use super::*;
use crate::offscreen::OffscreenTarget;

fn rebuilds_after(
    gpu: &SceneGpu,
    render: &mut dyn FnMut(&SceneGpu) -> crate::offscreen::RgbaImage,
) -> (crate::offscreen::RgbaImage, usize) {
    let rebuilds = |gpu: &SceneGpu| {
        gpu.views
            .iter()
            .map(|view| view.rebuilds.get())
            .sum::<usize>()
    };
    let before = rebuilds(gpu);
    let pixels = render(gpu);
    (pixels, rebuilds(gpu) - before)
}

#[test]
fn unchanged_views_reuse_their_selection_until_an_input_changes() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    let mut target = OffscreenTarget::new(
        context.device(),
        Viewport {
            width: TARGET,
            height: TARGET,
        },
    )
    .unwrap();
    let mut render = |gpu: &SceneGpu| target.render(&context, gpu, &pipelines).unwrap();
    gpu.set_frame(context.queue(), selection_frame(5_000));
    let (near, _) = rebuilds_after(&gpu, &mut render);
    assert_eq!(rebuilds_after(&gpu, &mut render), (near.clone(), 0));
    gpu.set_frame(context.queue(), selection_frame(50_000));
    let (_, moved) = rebuilds_after(&gpu, &mut render);
    assert_eq!(moved, 1);
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![0, 1]);
    gpu.set_frame(context.queue(), selection_frame(5_000));
    let handle = gpu.instance_handles().next().unwrap();
    gpu.set_instance_visible(handle, false).unwrap();
    let (_, hidden) = rebuilds_after(&gpu, &mut render);
    assert_eq!((hidden, gpu.view_stats().culled), (1, 1));
    gpu.set_instance_visible(handle, true).unwrap();
    assert_eq!(rebuilds_after(&gpu, &mut render), (near, 1));
    let away = SceneTransform {
        position_millimeters: [0, 0, 1_000_000],
        ..*gpu.instance(handle).unwrap().0.transform()
    };
    gpu.set_instance_transform(handle, away).unwrap();
    gpu.flush_instances(context.device(), context.submission())
        .unwrap();
    let (_, moved_away) = rebuilds_after(&gpu, &mut render);
    assert_eq!((moved_away, gpu.view_stats().culled), (1, 1));
}

#[test]
fn caster_fits_are_reused_until_a_placement_changes() {
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT);
    let mut gpu = SceneGpu::upload(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    let receiver = glam::Mat4::from_scale(glam::Vec3::splat(0.05));
    let fresh = |gpu: &SceneGpu| {
        crate::draw::visibility::caster_matrix(&gpu.instances, &gpu.mesh_selection, None, receiver)
    };
    let fit = gpu.caster_matrix(receiver);
    assert_eq!((fit, gpu.caster_matrix(receiver)), (fresh(&gpu), fit));
    assert_eq!(gpu.caster_fits.computed.get(), 1);
    let handle = gpu.instance_handles().next().unwrap();
    let toward = SceneTransform {
        position_millimeters: [0, 0, 30_000],
        ..*gpu.instance(handle).unwrap().0.transform()
    };
    gpu.set_instance_transform(handle, toward).unwrap();
    assert_eq!(gpu.caster_matrix(receiver), fresh(&gpu));
    assert_eq!(gpu.caster_fits.computed.get(), 2);
}
