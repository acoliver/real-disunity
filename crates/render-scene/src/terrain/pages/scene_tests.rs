use super::{
    composition::LayerSample,
    residency_tests::{context, settings},
    scene::{ScenePages, TerrainPageSource},
    selection::PageKey,
};
use worldrunner_render_gpu::{configuration::AllocationBudget, submission::SubmissionAuthority};

#[test]
fn terrain_pages_cancelled_scene_selection_and_source_failures_do_not_publish() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.maximum_selected_pages = 4;
    parts.maximum_depth = 1;
    let settings = worldrunner_render_plan::TerrainPageSettings::new(parts).unwrap();
    let source = TerrainPageSource {
        settings,
        slots: 5,
        budget: AllocationBudget(100_000),
        sample: Box::new(|_| {
            vec![LayerSample {
                color: [64, 128, 192, 255],
                normal: [0, 0, i16::MAX],
                roughness: 180,
                weight: 1,
            }]
        }),
    };
    let mut pages = ScenePages::new(device, &authority, source, [0.0, 0.0, 4.0, 4.0]).unwrap();
    for cancel in [true, false] {
        let mut frame = authority.begin(device).unwrap();
        pages
            .prepare(device, &mut frame, glam::Mat4::IDENTITY, [32, 32])
            .unwrap();
        assert!(pages.published_selection.borrow().is_none());
        if cancel {
            frame.finish().cancel();
            assert!(pages.cache.lookup(PageKey::ROOT, 0).is_none());
        } else {
            frame.finish().submit().unwrap();
            assert!(pages.published_selection.borrow().is_some());
            assert!(pages.cache.lookup(PageKey::ROOT, 0).is_some());
        }
    }
}

#[test]
fn terrain_pages_scene_source_errors_retain_the_composition_failure() {
    let context = context();
    let device = context.device();
    let authority = SubmissionAuthority::new(context.queue().clone());
    let mut parts = settings().parts();
    parts.maximum_selected_pages = 1;
    parts.maximum_depth = 0;
    let settings = worldrunner_render_plan::TerrainPageSettings::new(parts).unwrap();
    let source = TerrainPageSource {
        settings,
        slots: 1,
        budget: AllocationBudget(100_000),
        sample: Box::new(|_| Vec::new()),
    };
    let mut pages = ScenePages::new(device, &authority, source, [0.0, 0.0, 4.0, 4.0]).unwrap();
    let mut frame = authority.begin(device).unwrap();
    let error = pages
        .prepare(device, &mut frame, glam::Mat4::IDENTITY, [32, 32])
        .unwrap_err();
    assert!(error.to_string().contains("between one and 256 layers"));
    frame.finish().cancel();
    assert!(pages.cache.residents().is_empty());
    assert!(pages.published_selection.borrow().is_none());
}
