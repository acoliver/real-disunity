//! Same-frame shadow fitting must declare the exact contour allocation before its prefix.
use super::*;
use crate::draw::PreparedScene;
use crate::shadows::ShadowedTarget;
use crate::shadows::{DirectionalShadowSettings, ShadowedScene, sdsm};
use worldrunner_render_shadow::{MinimumSpanPolicy, RangeFit, SplitPolicy};

fn shadowed(context: &GpuContext, source: &RenderScene) -> ShadowedScene {
    let mut scene = ShadowedScene::new(
        context,
        PreparedScene::new(source).unwrap(),
        DirectionalShadowSettings {
            focus_meters: glam::Vec3::new(0.0, 0.0, 0.5),
            half_extent_meters: 4.0,
            depth_half_extent_meters: 4.0,
            resolution: 64,
            depth_bias: 0.001,
            normal_bias_meters: 0.01,
            refresh_policy: worldrunner_render_shadow::CascadeRefreshPolicy::default(),
        },
        ShadowedTarget::new(TextureFormat::Rgba16Float, 1),
    )
    .unwrap();
    scene
        .set_depth_fitting(
            context,
            Some(sdsm::SdsmSettings {
                policy: sdsm::AdoptionPolicy::SameFrameCapture,
                range: RangeFit {
                    near_step: 0.1,
                    far_step: 1.0,
                    minimum_span: 1.0,
                    minimum_span_policy: MinimumSpanPolicy::ExtendNear,
                    active_maximum: None,
                    split_lambda: 0.5,
                    split_policy: SplitPolicy::Practical,
                },
            }),
        )
        .unwrap();
    scene
}

struct CaptureRig {
    frame: FrameRenderer,
    scene: ShadowedScene,
    view: SceneView,
    color: AttachmentView,
    depth: AttachmentView,
    retained: Option<Texture>,
}

impl CaptureRig {
    fn new(context: &GpuContext) -> Self {
        let source = source_with_ids(255, false);
        let frame = Rig::new(context, &source, 1).renderer;
        let scene = shadowed(context, &source);
        let view = SceneView::new(
            pose(),
            camera(),
            Viewport {
                width: SIZE[0],
                height: SIZE[1],
            },
        )
        .unwrap();
        let color = AttachmentView::new(
            ResourceRef::new("scene color"),
            frame.targets().scene().texture(),
        )
        .unwrap();
        let depth = AttachmentView::new(
            ResourceRef::new("scene depth"),
            frame.targets().depth().texture(),
        )
        .unwrap();
        Self {
            frame,
            scene,
            view,
            color,
            depth,
            retained: None,
        }
    }

    fn check(&mut self, context: &GpuContext, index: usize, slot: usize, active: bool) {
        let Self {
            frame,
            scene,
            view,
            color,
            depth,
            retained,
        } = self;
        let handle = scene.scene_mut().instance_handles().next().unwrap();
        let object = scene.scene_mut().instance(handle).unwrap().0;
        frame.select_frame_slot(slot);
        scene.scene_mut().select_frame_slot(slot);
        scene
            .scene_mut()
            .set_instance_object(handle, object.with_contour_id(if active { 255 } else { 0 }))
            .unwrap();
        let source_frame = u64::try_from(index).unwrap();
        let fit = scene
            .render_depth_fitted_host(
                context,
                view,
                (color, depth),
                (Some(source_frame), false, &[]),
                frame,
            )
            .unwrap()
            .unwrap();
        assert_eq!(
            (fit.source_frame, fit.adopted_frame),
            (source_frame, source_frame)
        );
        let target = frame.contour_target();
        assert_eq!(target.is_some(), active);
        if index == 0 {
            *retained = target.clone();
        }
        if index == 1 {
            assert_eq!(target, *retained);
        }
        let mut encoder = context.submission().begin(context.device()).unwrap();
        let capture = frame
            .encode_contour_capture(context.device(), &mut encoder)
            .unwrap();
        assert_eq!(capture.is_some(), active);
        encoder.finish().submit().unwrap();
        if let Some(capture) = capture {
            let ids = read(context, &capture);
            assert!(ids.contains(&255));
            assert!(ids.contains(&0));
            assert!(ids.iter().all(|&id| id == 0 || id == 255));
        }
        if slot != 1 {
            frame.select_frame_slot(1);
            assert_eq!(frame.contour_target(), *retained);
        }
    }
}

#[test]
fn gpu_contour_shadowed_same_frame_capture_declares_active_and_empty_slots() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let context = context();
    let mut rig = CaptureRig::new(&context);
    for (index, (slot, active)) in [(1, true), (1, true), (2, false), (0, true), (0, false)]
        .into_iter()
        .enumerate()
    {
        rig.check(&context, index, slot, active);
    }
}
