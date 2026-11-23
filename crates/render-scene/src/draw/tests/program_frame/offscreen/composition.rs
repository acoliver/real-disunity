//! Public stock composition forwards offscreen work to the frame's executor before both draws.

mod omission;

use super::super::rig::{self, BLACK, Fixture, GREEN, Image, RED};
use super::{BLUE, CYAN, YELLOW, admit_scene_draw, build, check_offscreen_stages, player, timed};
use crate::compositor::prepare_stock_scene;
use worldrunner_render_post::contracts::RasterId;
use worldrunner_render_post::{
    AttachmentLoad, AttachmentView, BorrowedFeature, DrawContext, FeatureDeclaration, FeaturePass,
    FrameError, PassDeclaration, PassEntry, PassId, PassSlot, PrepareContext, PreparedFeature,
    ResourceRef,
};
use worldrunner_render_program::unity::runtime::AdmittedPrograms;

const STOCK: PassEntry = PassEntry::new(PassSlot::Opaque);
const CALLER: PassEntry = PassEntry::nth(PassSlot::Opaque, 1);

struct Caller(AdmittedPrograms);

impl PreparedFeature for Caller {
    fn prepare(&mut self, _: &mut PrepareContext<'_>) -> Result<(), FrameError> {
        // Fixture admission already retained these draws on this frame's encoder.
        Ok(())
    }

    fn draw(&self, context: &DrawContext, pass: &mut wgpu::RenderPass<'_>) {
        assert_eq!(context.entry, CALLER);
        pass.set_scissor_rect(54, 30, 4, 4);
        // The fixture's far blue patch lies entirely outside this scissor.
        self.0.record(pass);
    }
}

fn attachments(fixture: &Fixture) -> (AttachmentView, AttachmentView) {
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        fixture.renderer.targets().scene().texture(),
    )
    .unwrap();
    let depth = AttachmentView::new(
        ResourceRef::new("scene depth"),
        fixture.renderer.targets().depth().texture(),
    )
    .unwrap();
    (color, depth)
}

fn caller_pass(passes: &[PassDeclaration]) -> PassDeclaration {
    let stock = passes.iter().find(|pass| pass.entry() == STOCK).unwrap();
    let mut caller = PassDeclaration::nth(PassSlot::Opaque, 1);
    caller.color = stock.color.clone();
    caller.depth = stock.depth.clone();
    caller.color.as_mut().expect("stock color").load = AttachmentLoad::Load;
    caller.depth.as_mut().expect("stock depth").load = AttachmentLoad::Load;
    caller
}

fn caller_declaration(pass: &PassDeclaration) -> FeatureDeclaration {
    let mut feature = FeaturePass::nth(PassSlot::Opaque, 1);
    let color = pass.color.as_ref().expect("caller color");
    let depth = pass.depth.as_ref().expect("caller depth");
    feature.color_format = Some(color.format);
    feature.depth_format = Some(depth.format);
    feature.samples = color.samples;
    FeatureDeclaration {
        passes: vec![feature],
        effects: Vec::new(),
    }
}

fn encode_composed(
    fixture: &mut Fixture,
    encoder: &mut wgpu::CommandEncoder,
    caller: &mut Caller,
) -> Vec<RasterId> {
    let (color, depth) = attachments(fixture);
    let renderer = fixture.renderer.renderer_mut();
    let device = fixture.context.device();
    let mut stock = prepare_stock_scene(
        renderer,
        device,
        encoder,
        (&mut fixture.gpu, &fixture.pipelines),
        (&color, &depth),
        rig::frame(),
    )
    .unwrap();
    let plan = stock.plan();
    let (mut passes, views) = (plan.passes.to_vec(), plan.views.to_vec());
    let pass = caller_pass(&passes);
    let caller_declaration = caller_declaration(&pass);
    passes.push(pass);
    let (declaration, uniform) = (stock.declaration(), stock.frame());
    let Some(operations) = stock.take_offscreen_operations() else {
        panic!("stock preparation must hand off the admitted offscreen work");
    };
    assert!(stock.take_offscreen_operations().is_none());
    let ids = operations.ids().collect();
    // Eager work during stock preparation cannot appear in this frame's timing.
    renderer.begin_timing(8).unwrap();
    renderer
        .encode_frame(
            device,
            encoder,
            (&passes, &views),
            Some(&operations),
            &mut [
                BorrowedFeature {
                    declaration,
                    prepared: &mut stock,
                },
                BorrowedFeature {
                    declaration: caller_declaration,
                    prepared: caller,
                },
            ],
            &(uniform, None),
        )
        .unwrap();
    ids
}

fn check_pixels(image: &Image) {
    assert_eq!(image.pixel(42, 32), CYAN);
    assert_eq!(image.pixel(50, 32), BLUE);
    assert_eq!(image.pixel(55, 35), YELLOW, "outside the caller scissor");
    assert_eq!(image.pixel(55, 32), GREEN, "the later caller draw won");
    assert_eq!(image.pixel(30, 20), RED, "the stock card survived the Load");
    assert_eq!(image.pixel(3, 3), BLACK);
}

fn check_operation_names(ids: &[RasterId]) {
    let names: Vec<_> = ids.iter().map(|id| id.stage.name).collect();
    assert_eq!(
        names,
        [
            "offscreen constants",
            "offscreen camera",
            "offscreen blit",
            "offscreen copy",
            "offscreen readback",
            "offscreen readback",
            "offscreen readback",
            "offscreen readback",
        ]
    );
}

fn unique_position(listed: &[PassId], id: PassId) -> usize {
    let positions: Vec<_> = listed
        .iter()
        .enumerate()
        .filter(|(_, pass)| **pass == id)
        .map(|(index, _)| index)
        .collect();
    let [position] = positions.as_slice() else {
        panic!("{id:?} must execute once, got positions {positions:?}");
    };
    *position
}

/// Checks one timing record, preserving the offscreen names and prefix contract. Geometry
/// entries must appear exactly once, in the supplied order, after every admitted operation.
pub(super) fn check_timing(fixture: &Fixture, ids: &[RasterId], geometry: &[PassEntry]) {
    fixture
        .context
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    let timings = crate::test_support::take_timings(fixture.context.device(), 1, || {
        fixture.renderer.take_timings()
    });
    let [timing] = timings.as_slice() else {
        panic!("one timing record, got {}", timings.len());
    };
    check_timing_record(timing, ids, geometry);
}

pub(super) fn check_timing_record(
    timing: &worldrunner_render_post::FrameTiming,
    ids: &[RasterId],
    geometry: &[PassEntry],
) {
    check_operation_names(ids);
    let listed: Vec<_> = timing.passes.iter().map(|pass| pass.pass_id).collect();
    let expected: Vec<_> = ids.iter().map(|id| PassId::Operation(*id)).collect();
    assert_eq!(listed[..expected.len()], expected[..]);
    let actual_geometry: Vec<_> = listed
        .iter()
        .filter_map(|id| match id {
            PassId::Geometry(entry) => Some(*entry),
            _ => None,
        })
        .collect();
    assert_eq!(actual_geometry.as_slice(), geometry);
    let positions: Vec<_> = geometry
        .iter()
        .map(|entry| unique_position(&listed, PassId::Geometry(*entry)))
        .collect();
    assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
    for operation in expected {
        let position = unique_position(&listed, operation);
        assert!(positions.iter().all(|geometry| position < *geometry));
    }
}

#[test]
fn public_stock_composition_forwards_admitted_offscreen_work() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = timed();
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
    let signature = fixture.signature();
    let mut caller = Caller(fixture.admit(&recording, signature).unwrap());

    let ids = encode_composed(&mut fixture, &mut recording, &mut caller);
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
        "one composed frame and its output copy, one submission"
    );

    check_offscreen_stages(readbacks, &device);
    check_pixels(&fixture.read(&output));
    assert_eq!(
        authority.submitted_serial(),
        before + 1,
        "offscreen work, the scene and their readbacks use the composed submission"
    );
    check_timing(&fixture, &ids, &[STOCK, CALLER]);
    assert_eq!(
        authority.submitted_serial(),
        before + 2,
        "collecting completed-frame timestamps adds exactly one resolve submission"
    );
}
