use super::lod_material::{scene, selection_frame};
use super::*;
use crate::draw::PreparedTextureReplacement;
use crate::offscreen::OffscreenTarget;
use crate::test_support::upload_scene;

fn rebuilds_after(
    gpu: &mut SceneGpu,
    render: &mut dyn FnMut(&mut SceneGpu) -> crate::offscreen::RgbaImage,
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
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(
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
    let mut render = |gpu: &mut SceneGpu| target.render(&context, gpu, &pipelines).unwrap();
    gpu.set_frame(context.queue(), selection_frame(5_000));
    let (near, _) = rebuilds_after(&mut gpu, &mut render);
    assert_eq!(rebuilds_after(&mut gpu, &mut render), (near.clone(), 0));
    gpu.set_frame(context.queue(), selection_frame(50_000));
    let (_, moved) = rebuilds_after(&mut gpu, &mut render);
    assert_eq!(moved, 1);
    assert_eq!(gpu.view_stats().drawn_per_lod, vec![0, 1]);
    gpu.set_frame(context.queue(), selection_frame(5_000));
    let handle = gpu.instance_handles().next().unwrap();
    gpu.set_instance_visible(handle, false).unwrap();
    let (_, hidden) = rebuilds_after(&mut gpu, &mut render);
    assert_eq!((hidden, gpu.view_stats().culled), (1, 1));
    gpu.set_instance_visible(handle, true).unwrap();
    assert_eq!(rebuilds_after(&mut gpu, &mut render), (near, 1));
    let away = SceneTransform {
        position_millimeters: [0, 0, 1_000_000],
        ..*gpu.instance(handle).unwrap().0.transform()
    };
    gpu.set_instance_transform(handle, away).unwrap();
    gpu.flush_instances(context.device(), context.submission())
        .unwrap();
    let (_, moved_away) = rebuilds_after(&mut gpu, &mut render);
    assert_eq!((moved_away, gpu.view_stats().culled), (1, 1));
}

#[test]
fn caster_fits_are_reused_until_a_placement_changes() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(
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

/// Receivers of one frame, each reaching the quad at the origin at a different depth and
/// moving a little every frame, as the cascades of a walking camera do.
fn receivers(frame: u16) -> Vec<glam::Mat4> {
    (0..4_u16)
        .map(|cascade| {
            let depth = 0.2 + 0.1 * f32::from(cascade) + 0.01 * f32::from(frame);
            glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, -depth))
                * glam::Mat4::from_scale(glam::Vec3::splat(0.05))
        })
        .collect()
}

fn fitted(gpu: &SceneGpu, receivers: &[glam::Mat4]) -> Vec<[u32; 16]> {
    let mut fits = receivers.to_vec();
    gpu.caster_matrices(&mut fits);
    fits.iter()
        .map(|fit| fit.to_cols_array().map(f32::to_bits))
        .collect()
}

fn fresh_fits(gpu: &SceneGpu, receivers: &[glam::Mat4]) -> Vec<[u32; 16]> {
    receivers
        .iter()
        .map(|&receiver| {
            crate::draw::visibility::caster_matrix(
                &gpu.instances,
                &gpu.mesh_selection,
                None,
                receiver,
            )
            .to_cols_array()
            .map(f32::to_bits)
        })
        .collect()
}

#[test]
fn cascade_fits_follow_the_receivers_and_placements_frame_after_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    let computed = |gpu: &SceneGpu| gpu.caster_fits.computed.get();
    for frame in 0..5 {
        let receivers = receivers(frame);
        let before = computed(&gpu);
        assert_eq!(fitted(&gpu, &receivers), fresh_fits(&gpu, &receivers));
        assert_eq!(
            computed(&gpu) - before,
            4,
            "frame {frame}: a fit per cascade"
        );
        assert_eq!(fitted(&gpu, &receivers), fresh_fits(&gpu, &receivers));
        assert_eq!(
            computed(&gpu) - before,
            4,
            "frame {frame}: a repeat is reused"
        );
    }
    let receivers = receivers(4);
    let untouched = fresh_fits(&gpu, &receivers);
    assert_ne!(untouched[0], untouched[1], "the cascades fit differently");
    let handle = gpu.instance_handles().next().unwrap();
    let toward = SceneTransform {
        position_millimeters: [0, 0, -9_000],
        ..*gpu.instance(handle).unwrap().0.transform()
    };
    gpu.set_instance_transform(handle, toward).unwrap();
    let before = computed(&gpu);
    let edited = fitted(&gpu, &receivers);
    assert_eq!(edited, fresh_fits(&gpu, &receivers));
    assert_ne!(edited, untouched, "the moved caster changes the fits");
    assert_eq!(computed(&gpu) - before, 4, "an edit refits every cascade");
    // Fewer cascades than before reuse the fits made for the same receivers.
    assert_eq!(fitted(&gpu, &receivers[..2]), edited[..2]);
    assert_eq!(computed(&gpu) - before, 4);
    // A different receiver is fitted afresh whatever the cache holds.
    let next = receivers_after(&receivers);
    assert_eq!(fitted(&gpu, &next[..3]), fresh_fits(&gpu, &next[..3]));
    assert_eq!(computed(&gpu) - before, 7);
}

fn receivers_after(receivers: &[glam::Mat4]) -> Vec<glam::Mat4> {
    receivers
        .iter()
        .map(|&receiver| glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, -0.05)) * receiver)
        .collect()
}

#[test]
fn drawn_pairs_follow_the_placements_and_match_a_fresh_collection() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    let fresh = |gpu: &SceneGpu| gpu.collect_drawn_pairs().into_iter().collect::<Vec<_>>();
    let loaded = gpu.drawn_pairs().to_vec();
    assert!(!loaded.is_empty());
    assert_eq!(
        (gpu.drawn_pairs().to_vec(), fresh(&gpu)),
        (loaded.clone(), loaded.clone())
    );

    let handle = gpu.instance_handles().next().unwrap();
    let (object, _) = gpu.instance(handle).unwrap();
    gpu.remove_instance(handle).unwrap();
    assert!(
        gpu.drawn_pairs().is_empty(),
        "a batch without placements draws nothing"
    );
    assert_eq!(fresh(&gpu), Vec::new());

    // A placement with another material makes a batch again.
    let other = SceneObject::new(
        *object.transform(),
        object.mesh(),
        worldrunner_render_plan::MaterialHandle::from_raw(2),
    );
    gpu.add_instance(other, true).unwrap();
    let changed = gpu.drawn_pairs().to_vec();
    assert_eq!(changed, fresh(&gpu));
    assert!(!changed.is_empty(), "the new batch draws");
    gpu.add_instance(object, true).unwrap();
    assert_eq!(gpu.drawn_pairs().to_vec(), fresh(&gpu));
    assert_eq!(gpu.drawn_pairs().to_vec(), loaded);
}

#[test]
fn prepared_pair_lists_look_up_nothing_until_the_pairs_change() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let mut gpu = upload_scene(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    let objects = pipelines.object_context();
    gpu.prepare_object_context(objects).unwrap();
    let pairs = gpu.drawn_pairs().len();
    assert!(pairs > 0);
    let after_first = gpu.objects.lookups();
    assert_eq!(after_first, pairs, "one lookup per pair the first time");
    for _ in 0..4 {
        gpu.prepare_object_context(objects).unwrap();
    }
    assert_eq!(
        gpu.objects.lookups(),
        after_first,
        "the same list is not looked up again"
    );

    // Every pair of the list has its pipeline: asking for them builds nothing.
    let built = pipelines.object_runtime_stats();
    for &(index, material, geometry) in &*gpu.drawn_pairs() {
        gpu.objects
            .pipeline(objects, (index, material), geometry)
            .unwrap();
    }
    assert_eq!(pipelines.object_runtime_stats(), built);

    // Another list is looked up once, whatever it holds, and what it needs is made.
    let handle = gpu.instance_handles().next().unwrap();
    let (object, _) = gpu.instance(handle).unwrap();
    gpu.remove_instance(handle).unwrap();
    assert!(gpu.drawn_pairs().is_empty());
    let before = gpu.objects.lookups();
    gpu.prepare_object_context(objects).unwrap();
    assert_eq!(
        gpu.objects.lookups(),
        before,
        "an empty list has nothing to look up"
    );
    let other = SceneObject::new(
        *object.transform(),
        object.mesh(),
        worldrunner_render_plan::MaterialHandle::from_raw(2),
    );
    gpu.add_instance(other, true).unwrap();
    let changed = gpu.drawn_pairs().len();
    assert!(changed > 0);
    gpu.prepare_object_context(objects).unwrap();
    assert_eq!(gpu.objects.lookups() - before, changed);
    gpu.prepare_object_context(objects).unwrap();
    assert_eq!(gpu.objects.lookups() - before, changed);
    for &(index, material, geometry) in &*gpu.drawn_pairs() {
        let built = pipelines.object_runtime_stats();
        gpu.objects
            .pipeline(objects, (index, material), geometry)
            .unwrap();
        assert_eq!(pipelines.object_runtime_stats(), built, "already prepared");
    }
}

#[test]
fn replacing_a_materials_texture_prepares_the_pair_list_again() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let source = scene(true);
    let mut gpu =
        upload_scene(context.device(), context.submission(), &source, &pipelines).unwrap();
    let objects = pipelines.object_context();
    gpu.prepare_object_context(objects).unwrap();
    let pairs = gpu.drawn_pairs().len();
    assert!(pairs > 0);
    let before = gpu.objects.lookups();
    gpu.prepare_object_context(objects).unwrap();
    assert_eq!(gpu.objects.lookups(), before, "the list is remembered");

    let replacement =
        PreparedTextureReplacement::new(&source, TextureHandle::from_raw(0), &texture(0x07e0))
            .unwrap();
    gpu.replace_texture_prepared(
        context.device(),
        context.submission(),
        &pipelines,
        &replacement,
    )
    .unwrap();
    gpu.prepare_object_context(objects).unwrap();
    assert_eq!(
        gpu.objects.lookups() - before,
        pairs,
        "the pipelines bind the old texture, so every pair is prepared again"
    );
    gpu.prepare_object_context(objects).unwrap();
    assert_eq!(gpu.objects.lookups() - before, pairs);
}

#[test]
fn a_context_prepared_for_a_list_is_prepared_on_its_own_and_dropped_with_it() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let gpu = upload_scene(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    let first = pipelines.object_context();
    let other = pipelines.for_raster(
        worldrunner_render_plan::SceneSpecularConvention::default(),
        wgpu::FrontFace::Cw,
    );
    gpu.prepare_object_context(first).unwrap();
    let pairs = gpu.drawn_pairs().len();
    let before = gpu.objects.lookups();
    gpu.prepare_object_context(other.object_context()).unwrap();
    assert_eq!(
        gpu.objects.lookups() - before,
        pairs,
        "another context has no pipelines yet"
    );
    gpu.prepare_object_context(first).unwrap();
    gpu.prepare_object_context(other.object_context()).unwrap();
    assert_eq!(
        gpu.objects.lookups() - before,
        pairs,
        "both lists are remembered"
    );
}

#[test]
fn passes_share_one_selection_of_a_view_instead_of_copying_it() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let pipelines = ScenePipelines::new(context.device(), COLOR_FORMAT, 1).unwrap();
    let gpu = upload_scene(
        context.device(),
        context.submission(),
        &scene(true),
        &pipelines,
    )
    .unwrap();
    gpu.set_frame(context.queue(), selection_frame(5_000));
    let frame = gpu.drawing_uniform();
    let first = gpu.select_objects(None, &frame);
    let again = gpu.select_objects(None, &frame);
    assert!(
        std::rc::Rc::ptr_eq(&first, &again),
        "an unchanged view lends the selection it built"
    );
    assert!(!first.ranges.is_empty());
    gpu.set_frame(context.queue(), selection_frame(50_000));
    let moved = gpu.select_objects(None, &gpu.drawing_uniform());
    assert!(!std::rc::Rc::ptr_eq(&first, &moved));
    assert_ne!(first.stats.drawn_per_lod, moved.stats.drawn_per_lod);
}
