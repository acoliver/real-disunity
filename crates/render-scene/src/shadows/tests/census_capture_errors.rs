use super::super::{ShadowError, ShadowedScene};
use super::{
    context,
    fixture::{scene, settings, view},
};
use crate::draw::PreparedScene;
use crate::offscreen::OffscreenTarget;
use crate::shadows::ShadowedTarget;
use crate::{
    Viewport,
    compositor::terms::{CaptureTerm, TermCapture},
    test_support::SceneTestSession,
};
use worldrunner_render_gpu::submission::{SubmitError, TransactionError};

#[test]
fn capture_without_transaction_reports_missing() {
    assert_transaction_error(false);
}

#[test]
fn capture_with_foreign_encoder_reports_foreign_encoder() {
    assert_transaction_error(true);
}

fn assert_transaction_error(foreign: bool) {
    let _session = SceneTestSession::acquire();
    let context = context();
    let source = scene(&[[0, 0, 1_000]], 600_000, None);
    let renderer = ShadowedScene::new(
        &context,
        PreparedScene::new(&source).unwrap(),
        settings(),
        ShadowedTarget::new(OffscreenTarget::COLOR_FORMAT, 1),
    )
    .unwrap();
    let camera = view()
        .with_raster_sample(
            Viewport {
                width: 64,
                height: 64,
            },
            [0.0; 2],
        )
        .unwrap();
    let scheduler = super::census_capture::capture_scheduler(&context, &camera);
    let capture = TermCapture::new(context.device(), [64, 64], CaptureTerm::SunVisibility);
    let mut encoder = context
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let expected = if foreign {
        TransactionError::ForeignEncoder
    } else {
        TransactionError::Missing
    };
    let mut check = || {
        assert!(matches!(
            renderer.encode_term_capture(context.device(), &mut encoder, &capture, &scheduler),
            Err(ShadowError::Submission(SubmitError::Transaction(actual))) if actual == expected
        ));
    };
    if foreign {
        context
            .submission()
            .prepare(context.device(), |_, _| {
                check();
                Ok::<_, ShadowError>(())
            })
            .unwrap()
            .cancel();
    } else {
        check();
    }
}
