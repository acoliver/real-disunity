use super::{Products, Refusal, Scene};
use worldrunner_render_gpu::submission::capture::{CaptureError, CaptureOutcome, CaptureWrites};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_scene::{
    compositor::{StockDepthPrefix, prepare_stock_scene},
    device::GpuContext,
    executor::{
        AttachmentView, BorrowedFeature, FrameError, FramePlan, PassDeclaration, PassEntry,
        PassSlot, ResourceRef,
    },
    wgpu,
};

pub(super) fn assert_retry(context: &GpuContext, source: &RenderScene) {
    let mut scene = Scene::new(context, source);
    let refusal = Refusal::default();
    scene
        .frame
        .register(context.device(), Box::new(refusal.clone()))
        .unwrap();
    scene.complete(context);
    assert_eq!(refusal.preparations(), 1);
    Products::read(context, &scene).assert_confidence(0.0);
    scene.seed_confidence(context);
    let completed = Products::read(context, &scene);
    completed.assert_confidence(0.5);
    let prefix = refuse(context, &mut scene, &refusal, &completed);
    refusal.set_refusing(false);
    scene.complete(context);
    assert_eq!(refusal.preparations(), 3);
    let mut control = Scene::new(context, source);
    control.complete(context);
    control.seed_confidence(context);
    Products::read(context, &control).assert_confidence(0.5);
    control.move_retained_instance();
    control.complete(context);
    let retried = assert_matches(context, &scene, &control);
    prefix.assert_prefix_matches(&retried);
    retried.assert_instance_motion(&completed);
    retried.assert_confidence(0.0);
    scene.seed_confidence(context);
    control.seed_confidence(context);
    Products::read(context, &scene).assert_confidence(0.5);
    Products::read(context, &control).assert_confidence(0.5);
    scene.complete(context);
    control.complete(context);
    assert_eq!(refusal.preparations(), 4);
    let settled = assert_matches(context, &scene, &control);
    settled.assert_motion_settled();
    settled.assert_confidence(0.0);
}

fn refuse(
    context: &GpuContext,
    scene: &mut Scene,
    refusal: &Refusal,
    completed: &Products,
) -> Products {
    let statistics = scene.gpu.statistics();
    let selection = scene.gpu.view_stats();
    let jitter = scene.frame.projection_jitter_index();
    let history = scene.frame.has_temporal_history();
    scene.move_retained_instance();
    capture_refusal(context, scene, refusal);
    assert_eq!(refusal.preparations(), 2);
    assert_eq!(scene.gpu.statistics(), statistics);
    assert_eq!(scene.gpu.view_stats(), selection);
    assert_eq!(scene.frame.projection_jitter_index(), jitter);
    assert_eq!(scene.frame.has_temporal_history(), history);
    let refused = Products::read(context, scene);
    refused.assert_completed_unchanged(completed);
    refused.assert_instance_motion(completed);
    refused
}

fn assert_matches(context: &GpuContext, scene: &Scene, control: &Scene) -> Products {
    assert_eq!(scene.gpu.statistics(), control.gpu.statistics());
    assert_eq!(scene.gpu.view_stats(), control.gpu.view_stats());
    assert_eq!(
        scene.frame.projection_jitter_index(),
        control.frame.projection_jitter_index()
    );
    assert_eq!(
        scene.frame.has_temporal_history(),
        control.frame.has_temporal_history()
    );
    let actual = Products::read(context, scene);
    assert_eq!(actual, Products::read(context, control));
    actual
}

fn capture_refusal(context: &GpuContext, scene: &mut Scene, refusal: &Refusal) {
    let device = context.device().clone();
    let authority = context.submission().clone();
    let prefix = scene.frame.stock_depth_prefix(&device, &scene.gpu).unwrap();
    assert_prefix_plan(&prefix);
    let writes = capture_writes(scene, &device, &prefix);
    let uniform = scene.uniform;
    let submitted = authority.submitted_serial();
    let first = authority
        .begin_capture(&device, writes, (), |encoder, _, ()| {
            prefix.encode(
                &mut scene.frame,
                &device,
                encoder,
                (&mut scene.gpu, &scene.pipelines),
                uniform,
                (&empty_plan(), &mut []),
            )
        })
        .unwrap();
    let continuation = first.submit().unwrap();
    let observer = continuation.observer();
    refusal.set_refusing(true);
    let resumed = continuation.resume(
        &device,
        |_, _| Ok::<_, FrameError>(()),
        |encoder, _, (), ()| encode_suffix(scene, &device, encoder),
    );
    assert_eq!(
        authority.submitted_serial(),
        submitted + 1,
        "only the physical stock prefix submitted"
    );
    match resumed {
        Err(CaptureError::Preparation(error)) => assert_eq!(error, Refusal::error()),
        Err(error) => panic!("unexpected capture failure: {error:?}"),
        Ok(_) => panic!("registered caller unexpectedly accepted the suffix"),
    }
    assert_eq!(observer.outcome(), Some(CaptureOutcome::PreparationFailed));
    authority.drain_captures(&device).unwrap();
}

fn assert_prefix_plan(prefix: &StockDepthPrefix) {
    assert_eq!(
        prefix
            .plan()
            .passes
            .iter()
            .map(PassDeclaration::entry)
            .collect::<Vec<_>>(),
        vec![
            PassEntry::new(PassSlot::DepthNormalPrepass),
            PassEntry::new(PassSlot::DepthPyramid(0)),
        ]
    );
}

fn capture_writes(
    scene: &mut Scene,
    device: &wgpu::Device,
    prefix: &StockDepthPrefix,
) -> CaptureWrites {
    let scene_keys = scene.gpu.capture_write_keys(device).unwrap();
    let plan = prefix.plan();
    let written: Vec<&wgpu::Texture> = plan.views.iter().map(AttachmentView::texture).collect();
    let metadata = scene.frame.capture_texture_keys(&written).unwrap();
    let color = scene.frame.targets().scene().texture().clone();
    let depth = scene.frame.targets().depth().texture().clone();
    let velocity = scene.frame.targets().velocity_texture().cloned().unwrap();
    let shared = scene
        .frame
        .capture_texture_keys(&[&depth, &velocity])
        .unwrap();
    let mut suffix = scene_keys.clone();
    suffix.extend(scene.frame.capture_write_keys());
    suffix.extend(scene.frame.capture_effect_keys(device).unwrap());
    suffix.extend(
        scene
            .frame
            .capture_texture_keys(&[&color, &depth, &velocity])
            .unwrap(),
    );
    // Depth and velocity remain shared; the other prefix attachments are complete.
    suffix.retain(|key| shared.contains(key) || !metadata.contains(key));
    let mut first = scene_keys;
    first.extend(metadata);
    let suffix_owned: Vec<_> = first
        .iter()
        .copied()
        .filter(|key| suffix.contains(key))
        .collect();
    CaptureWrites::new(first, suffix, suffix_owned).unwrap()
}

fn encode_suffix(
    scene: &mut Scene,
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
) -> Result<(), FrameError> {
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        scene.frame.targets().scene().texture(),
    )?;
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        scene.frame.targets().depth().texture(),
    )?;
    let mut stock = prepare_stock_scene(
        &mut scene.frame,
        device,
        encoder,
        (&mut scene.gpu, &scene.pipelines),
        (&color, &depth),
        scene.uniform,
    )?;
    let plan = stock.plan();
    let (passes, views) = (plan.passes.to_vec(), plan.views.to_vec());
    let (declaration, frame) = (stock.declaration(), stock.frame());
    scene.frame.encode_frame(
        device,
        encoder,
        (&passes, &views),
        &mut [BorrowedFeature {
            declaration,
            prepared: &mut stock,
        }],
        (frame, None),
    )
}

fn empty_plan() -> FramePlan<'static> {
    FramePlan {
        passes: &[],
        views: &[],
        requested: &[],
    }
}
