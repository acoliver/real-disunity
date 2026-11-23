//! A cached caller binding must keep the retained-content permission of its current view.
use super::support::{Counting, Feature, declaring};
use crate::{
    CameraPose, SceneView, Viewport,
    compositor::{FrameRenderer, attachment},
    device::GpuContext,
    frame::FrameUniform,
};
use worldrunner_render_gpu::submission::PrepareError;
use worldrunner_render_plan::SceneCamera;
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, BorrowedFeature, FrameError, PassDeclaration, PassEntry,
    PassSlot, ResourceRef, ToneMap, color_texture,
};

const SIZE: u32 = 16;
const OUTPUT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

struct Attempt {
    result: Result<(), FrameError>,
    prepared: u32,
    serial_before: u64,
    serial_after: u64,
}

fn frame() -> FrameUniform {
    let view = SceneView::new(
        CameraPose {
            eye_millimeters: [0, -10_000, 1_700],
            target_millimeters: [0, 0, 1_700],
        },
        SceneCamera {
            vertical_fov_millidegrees: 60_000,
            near_plane_millimeters: 100,
            far_plane_millimeters: 100_000,
        },
        Viewport {
            width: SIZE,
            height: SIZE,
        },
    )
    .unwrap();
    FrameUniform::new(&view, None).unwrap()
}

fn renderer(context: &GpuContext) -> FrameRenderer {
    let mut renderer =
        FrameRenderer::hdr(context, [SIZE, SIZE], OUTPUT, 1, ToneMap::Reinhard, None).unwrap();
    renderer.select_frame_slot(0);
    renderer
}

fn initialize(context: &GpuContext, texture: &wgpu::Texture) {
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let transaction = context
        .submission()
        .prepare(context.device(), |encoder, _| {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("initialize retained-permission target"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            Ok::<_, FrameError>(())
        })
        .unwrap();
    transaction.submit().unwrap();
}

fn attempt(
    context: &GpuContext,
    renderer: &mut FrameRenderer,
    view: &AttachmentView,
    retained: bool,
) -> Attempt {
    let device = context.device();
    let selected_view = if retained {
        view.clone().with_retained_contents()
    } else {
        view.clone()
    };
    let resource = selected_view.resource();
    let mut pass = PassDeclaration::new(PassSlot::Opaque);
    let mut attachment = attachment(resource, &selected_view, false);
    attachment.load = AttachmentLoad::Load;
    pass.color = Some(attachment);
    let entry = PassEntry::new(PassSlot::Opaque);
    let counting = Counting::default();
    let prepared_count = counting.prepared.clone();
    let mut feature = Feature(counting);
    let mut features = [BorrowedFeature {
        declaration: declaring(&[entry]),
        prepared: &mut feature,
    }];
    let views = [selected_view];
    let prepared = renderer.prepare_uniform(frame()).unwrap();
    let serial_before = context.submission().submitted_serial();
    let result = match context.submission().prepare(device, |encoder, _| {
        renderer.encode_frame(
            device,
            encoder,
            (&[pass], &views),
            None,
            &mut features,
            &(prepared, None),
        )
    }) {
        Ok(transaction) => {
            transaction.submit().unwrap();
            Ok(())
        }
        Err(PrepareError::Preparation(error)) => Err(error),
        Err(other) => panic!("submission did not begin: {other:?}"),
    };
    Attempt {
        result,
        prepared: prepared_count.get(),
        serial_before,
        serial_after: context.submission().submitted_serial(),
    }
}

fn fixture() -> (GpuContext, AttachmentView) {
    let context = GpuContext::shared_for_test().unwrap();
    let texture = color_texture(
        context.device(),
        "retained permission target",
        [SIZE, SIZE],
        wgpu::TextureFormat::Rgba16Float,
    );
    initialize(&context, &texture);
    let view =
        AttachmentView::new(ResourceRef::new("retained permission target"), &texture).unwrap();
    (context, view)
}

#[test]
fn retained_to_unretained_rebind_refuses_a_load_without_a_producer() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (context, view) = fixture();
    let mut cached = renderer(&context);
    let before_warm = cached.stock_plan_statistics();

    assert_eq!(attempt(&context, &mut cached, &view, true).result, Ok(()));
    assert_eq!(attempt(&context, &mut cached, &view, true).result, Ok(()));
    let after_warm = cached.stock_plan_statistics();
    assert_eq!(after_warm.reuses, before_warm.reuses + 1);
    assert_eq!(
        after_warm.binding_resolutions,
        before_warm.binding_resolutions + 1,
        "two successful frames on explicit slot zero resolved once and reused the bound table"
    );

    let cached_attempt = attempt(&context, &mut cached, &view, false);
    let mut control = renderer(&context);
    let fresh_attempt = attempt(&context, &mut control, &view, false);
    let missing = Err(FrameError::MissingProducer(view.resource()));
    assert_eq!(fresh_attempt.result, missing);
    assert_eq!(cached_attempt.result, missing);
    assert_eq!(cached_attempt.prepared, 0);
    assert_eq!(fresh_attempt.prepared, 0);
    assert_eq!(cached_attempt.serial_after, cached_attempt.serial_before);
    assert_eq!(fresh_attempt.serial_after, fresh_attempt.serial_before);
}

#[test]
fn unretained_to_retained_rebind_accepts_the_current_initialization_permission() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let (context, view) = fixture();
    let mut cached = renderer(&context);
    let cold = attempt(&context, &mut cached, &view, false);
    assert_eq!(
        cold.result,
        Err(FrameError::MissingProducer(view.resource()))
    );
    assert_eq!(cold.prepared, 0);
    assert_eq!(cold.serial_after, cold.serial_before);
    let after_cold = cached.stock_plan_statistics();
    assert_eq!(after_cold.binding_resolutions, 1);

    let retained = attempt(&context, &mut cached, &view, true);
    let mut control = renderer(&context);
    let fresh = attempt(&context, &mut control, &view, true);
    assert_eq!(fresh.result, Ok(()));
    assert_eq!(fresh.prepared, 1);
    assert_eq!(fresh.serial_after, fresh.serial_before + 1);
    assert_eq!(retained.result, Ok(()));
    assert_eq!(retained.prepared, 1);
    assert_eq!(retained.serial_after, retained.serial_before + 1);
    assert_eq!(
        cached.stock_plan_statistics().binding_resolutions,
        after_cold.binding_resolutions + 1,
        "permission changes rebind the schedule instead of reusing a stale view table"
    );
}
