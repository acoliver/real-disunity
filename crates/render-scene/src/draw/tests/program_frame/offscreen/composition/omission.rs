//! Omitting the caller's offscreen forwarding leaves the stock draw with an unwritten texture.

use super::super::super::rig::{Fixture, RED};
use super::super::{SCENE_FORMAT, ScenePipelines, admit_scene_draw, build, player, settings};
use super::attachments;
use crate::compositor::prepare_stock_scene;
use worldrunner_render_post::BorrowedFeature;

#[derive(Clone, Copy)]
enum Omission {
    LeaveInStock,
    TakeButDoNotForward,
}

fn encode_without_forwarding(
    fixture: &mut Fixture,
    encoder: &mut wgpu::CommandEncoder,
    omission: Omission,
) {
    let (color, depth) = attachments(fixture);
    let device = fixture.context.device();
    let mut stock = prepare_stock_scene(
        fixture.renderer.renderer_mut(),
        device,
        encoder,
        (&mut fixture.gpu, &fixture.pipelines),
        (&color, &depth),
        super::super::super::rig::frame(),
    )
    .unwrap();
    let plan = stock.plan();
    let (passes, views) = (plan.passes.to_vec(), plan.views.to_vec());
    let (declaration, uniform) = (stock.declaration(), stock.frame());
    let omitted = match omission {
        Omission::LeaveInStock => None,
        Omission::TakeButDoNotForward => Some(
            stock
                .take_offscreen_operations()
                .expect("admitted offscreen work remains available to the caller"),
        ),
    };
    fixture
        .renderer
        .renderer_mut()
        .encode_frame(
            device,
            encoder,
            (&passes, &views),
            None,
            &mut [BorrowedFeature {
                declaration,
                prepared: &mut stock,
            }],
            &(uniform, None),
        )
        .unwrap();
    drop(omitted);
}

fn characterize_omission(omission: Omission) {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = Fixture::new(settings(), |device| {
        ScenePipelines::new(device, SCENE_FORMAT, 1).unwrap()
    });
    let mut player = player(&mut fixture);
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let before = authority.submitted_serial();
    let (operations, readbacks) = build(&mut player);
    let mut recording = authority.begin(&device).unwrap();
    fixture
        .gpu
        .admit_offscreen(operations.admit(&recording).unwrap());
    admit_scene_draw(&mut fixture, &mut player, &recording);
    encode_without_forwarding(&mut fixture, &mut recording, omission);
    let output = fixture.output_copy(&mut recording);
    assert_eq!(
        authority.submitted_serial(),
        before,
        "recording must not submit"
    );

    recording.finish().submit().unwrap();
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "caller submits once"
    );
    let image = fixture.read(&output);
    let omitted_pixel = image.pixel(42, 32);
    assert_eq!(
        &omitted_pixel[..3],
        &[0, 0, 0],
        "omitted work leaves the sampled RGB unwritten"
    );
    assert_eq!(image.pixel(30, 20), RED, "the stock scene draw still ran");
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "reading submits nothing"
    );
    drop(readbacks);
}

#[test]
fn leaving_admitted_work_in_stock_does_not_forward_it_automatically() {
    characterize_omission(Omission::LeaveInStock);
}

#[test]
fn taking_admitted_work_without_forwarding_does_not_encode_it() {
    characterize_omission(Omission::TakeButDoNotForward);
}
