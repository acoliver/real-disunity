//! Targets of several color textures: a `SetRenderTarget` that names them and a camera set up with
//! buffers. The worlds are synthetic; each test predicts the records and the reasons.

use glam::{Quat, Vec3};
use worldrunner_render_program::unity::offscreen::frame::{Attachment, Attachments, Clear, Record};
use worldrunner_world_simulation::graphics::{
    CameraBuffers, OffscreenWork, RenderTextureId, TargetRef,
};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentDesc, ComponentHandle, World};

use super::super::frame::{Adapted, Settings, Skip, adapt};
use super::super::ids;
use super::support::{available, object, render_texture, snapshot, world};

fn run(world: &mut World) -> Adapted {
    let snapshot = snapshot(world);
    adapt(
        &snapshot,
        &available(&snapshot, 1, &[]),
        &Settings::default(),
    )
}

fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

fn target(color: &[TargetRef], depth: Option<TargetRef>) -> OffscreenWork {
    OffscreenWork::SetRenderTarget {
        color: color.to_vec(),
        depth,
        mip: 0,
        face: -1,
        slice: -1,
    }
}

fn clear_all() -> OffscreenWork {
    OffscreenWork::ClearRenderTarget {
        color: true,
        depth: true,
        stencil: false,
        value: [0.25, 0.5, 0.75, 1.0],
        depth_value: 1.0,
        stencil_value: 0,
    }
}

fn camera(world: &mut World) -> ComponentHandle {
    let object = object(world, 0, Vec3::ZERO, Quat::IDENTITY);
    world
        .add_component(object, ComponentDesc::Camera(Camera::default()))
        .unwrap()
}

fn setup(
    camera: ComponentHandle,
    target: TargetRef,
    buffers: Option<CameraBuffers>,
) -> OffscreenWork {
    OffscreenWork::SetupCamera {
        camera,
        view: glam::Mat4::IDENTITY.to_cols_array(),
        projection: glam::Mat4::IDENTITY.to_cols_array(),
        target,
        buffers,
    }
}

fn whole(id: RenderTextureId) -> Attachment {
    Attachment {
        texture: ids::render_texture(id),
        mip: 0,
    }
}

fn clears(adapted: &Adapted) -> Vec<Attachments> {
    adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Clear { target, .. } => Some(target.clone()),
            _ => None,
        })
        .collect()
}

/// Four textures of one size: three to draw colors into and one for depth.
fn four(world: &mut World) -> [RenderTextureId; 4] {
    [(); 4].map(|()| render_texture(world, [8, 8]))
}

#[test]
fn a_target_of_several_colors_keeps_every_one_in_order_and_its_depth() {
    let mut world = world();
    let [a, b, c, depth] = four(&mut world);
    let colors = [a, b, c].map(TargetRef::Render);
    record(&mut world, target(&colors, Some(TargetRef::Render(depth))));
    record(&mut world, clear_all());
    let adapted = run(&mut world);
    assert_eq!(
        clears(&adapted),
        [Attachments {
            colors: vec![whole(a), whole(b), whole(c)],
            depth: Some(whole(depth)),
        }]
    );
    assert!(
        !adapted
            .report
            .census
            .keys()
            .any(|reason| reason.contains("left out")),
        "nothing is counted as left out: {:?}",
        adapted.report.census
    );
}

#[test]
fn several_colors_without_a_depth_target_use_the_depth_of_the_first() {
    let mut world = world();
    let [a, b, ..] = four(&mut world);
    record(&mut world, target(&[a, b].map(TargetRef::Render), None));
    record(&mut world, clear_all());
    let adapted = run(&mut world);
    assert_eq!(
        clears(&adapted),
        [Attachments {
            colors: vec![whole(a), whole(b)],
            depth: Some(whole(a)),
        }]
    );
}

#[test]
fn a_camera_set_up_with_buffers_draws_into_all_of_them_at_once() {
    let mut world = world();
    let [a, b, c, depth] = four(&mut world);
    let camera = camera(&mut world);
    let buffers = CameraBuffers::new(&[a, b, c], Some(depth)).unwrap();
    record(
        &mut world,
        setup(camera, TargetRef::CameraTarget, Some(buffers)),
    );
    record(&mut world, clear_all());
    // A pipeline binds the camera target by name, which stands for the buffers.
    record(&mut world, target(&[TargetRef::CameraTarget], None));
    record(&mut world, clear_all());
    let adapted = run(&mut world);
    let expected = Attachments {
        colors: vec![whole(a), whole(b), whole(c)],
        depth: Some(whole(depth)),
    };
    assert_eq!(clears(&adapted), [expected.clone(), expected]);
    assert!(
        adapted.report.skipped.is_empty(),
        "{:?}",
        adapted.report.skipped
    );
}

#[test]
fn camera_buffers_without_a_depth_texture_use_the_depth_of_the_first_color() {
    let mut world = world();
    let [a, b, ..] = four(&mut world);
    let camera = camera(&mut world);
    let buffers = CameraBuffers::new(&[a, b], None).unwrap();
    record(
        &mut world,
        setup(camera, TargetRef::CameraTarget, Some(buffers)),
    );
    record(&mut world, clear_all());
    let adapted = run(&mut world);
    assert_eq!(
        clears(&adapted),
        [Attachments {
            colors: vec![whole(a), whole(b)],
            depth: Some(whole(a)),
        }]
    );
}

#[test]
fn a_later_setup_with_a_target_texture_replaces_the_buffers() {
    let mut world = world();
    let [a, b, c, _] = four(&mut world);
    let camera = camera(&mut world);
    let buffers = CameraBuffers::new(&[a, b], None).unwrap();
    record(
        &mut world,
        setup(camera, TargetRef::CameraTarget, Some(buffers)),
    );
    record(&mut world, setup(camera, TargetRef::Render(c), None));
    record(&mut world, clear_all());
    // The camera target now names the one texture.
    record(&mut world, target(&[TargetRef::CameraTarget], None));
    record(&mut world, clear_all());
    let adapted = run(&mut world);
    let only = Attachments::single(Some(whole(c)), Some(whole(c)));
    assert_eq!(clears(&adapted), [only.clone(), only]);
}

#[test]
fn colors_that_a_pass_cannot_draw_into_together_are_skipped_by_name() {
    let mut world = world();
    let [a, b, ..] = four(&mut world);
    let small = render_texture(&mut world, [4, 4]);
    let camera = camera(&mut world);
    let eight = [(); 8].map(|()| render_texture(&mut world, [8, 8]));
    record(&mut world, target(&[a, a].map(TargetRef::Render), None));
    record(&mut world, target(&[a, small].map(TargetRef::Render), None));
    record(
        &mut world,
        target(&[TargetRef::None, TargetRef::Render(b)], None),
    );
    // The camera's eight buffers and one more are nine colors.
    let buffers = CameraBuffers::new(&eight, None).unwrap();
    record(
        &mut world,
        setup(camera, TargetRef::CameraTarget, Some(buffers)),
    );
    record(
        &mut world,
        target(&[TargetRef::CameraTarget, TargetRef::Render(a)], None),
    );
    let adapted = run(&mut world);
    for (skip, count) in [
        (Skip::RepeatedColorTarget, 1),
        (Skip::ColorSizeMismatch, 1),
        (Skip::EmptyColorSlot, 1),
        (Skip::TooManyColorTargets, 1),
    ] {
        assert_eq!(adapted.report.skipped.get(&skip), Some(&count), "{skip:?}");
    }
}

#[test]
fn buffers_that_cannot_be_drawn_into_together_leave_the_draws_with_no_target() {
    let mut world = world();
    let [a, ..] = four(&mut world);
    let small = render_texture(&mut world, [4, 4]);
    let camera = camera(&mut world);
    record(&mut world, target(&[TargetRef::Render(a)], None));
    let buffers = CameraBuffers::new(&[a, small], None).unwrap();
    record(
        &mut world,
        setup(camera, TargetRef::CameraTarget, Some(buffers)),
    );
    record(&mut world, clear_all());
    let adapted = run(&mut world);
    assert!(clears(&adapted).is_empty(), "the clear has nowhere to go");
    assert_eq!(
        adapted.report.skipped.get(&Skip::ColorSizeMismatch),
        Some(&1)
    );
    assert_eq!(adapted.report.skipped.get(&Skip::UnboundTarget), Some(&1));
}

#[test]
fn a_depth_texture_of_another_size_is_counted_as_left_out() {
    let mut world = world();
    let [a, ..] = four(&mut world);
    let small = render_texture(&mut world, [4, 4]);
    record(
        &mut world,
        target(&[TargetRef::Render(a)], Some(TargetRef::Render(small))),
    );
    let adapted = run(&mut world);
    assert_eq!(
        adapted.report.census["depth target left out: its size differs from the color's"],
        1
    );
}

#[test]
fn a_clear_in_every_target_clears_all_the_colors_with_one_value() {
    let mut world = world();
    let [a, b, c, _] = four(&mut world);
    record(&mut world, target(&[a, b, c].map(TargetRef::Render), None));
    record(&mut world, clear_all());
    let adapted = run(&mut world);
    let [Record::Clear { clear, .. }] = adapted.frame.records.as_slice() else {
        panic!("one clear: {:?}", adapted.frame.records);
    };
    assert_eq!(
        *clear,
        Clear {
            color: Some([0.25, 0.5, 0.75, 1.0]),
            depth: Some(1.0),
            stencil: None,
        }
    );
}
