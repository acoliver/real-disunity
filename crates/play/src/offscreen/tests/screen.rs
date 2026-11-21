//! The screen: what a command buffer's `CameraTarget` draws into when no camera texture names it.
//! With a screen set in the settings, the frame holds one render texture of the screen's size, with
//! a colour format and depth bits like a camera's render texture, that persists from frame to frame,
//! and the records that name the camera target bind it for colour and for depth.
//! Without one such a record is left out, as it was before the screen existed.

use glam::{Mat4, Quat, Vec3};
use worldrunner_render_program::unity::offscreen::frame::{
    Attachment, Attachments, Record, RenderDimension, RenderTextureSpec,
};
use worldrunner_world_simulation::graphics::{OffscreenWork, TargetRef};
use worldrunner_world_simulation::presentation::Camera;
use worldrunner_world_simulation::{ComponentDesc, ComponentHandle, World};

use super::super::frame::{Adapted, Settings, Skip, adapt};
use super::super::ids;
use super::support::{available, object, render_texture, snapshot, world};

const SIZE: [u32; 2] = [8, 6];

fn run(world: &mut World, screen: Option<[u32; 2]>) -> Adapted {
    let snapshot = snapshot(world);
    adapt(
        &snapshot,
        &available(&snapshot, 1, &[]),
        &Settings {
            screen,
            ..Settings::default()
        },
    )
}

fn record(world: &mut World, work: OffscreenWork) {
    world.record_offscreen(work).unwrap();
}

fn bind(color: &[TargetRef], depth: Option<TargetRef>) -> OffscreenWork {
    OffscreenWork::SetRenderTarget {
        color: color.to_vec(),
        depth,
        mip: 0,
        face: -1,
        slice: -1,
    }
}

fn clear_all() -> OffscreenWork {
    clear_with_color([0.25, 0.5, 0.75, 1.0])
}

fn clear_with_color(value: [f32; 4]) -> OffscreenWork {
    OffscreenWork::ClearRenderTarget {
        color: true,
        depth: true,
        stencil: false,
        value,
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

fn set_up(world: &mut World, camera: ComponentHandle, target: TargetRef) {
    record(
        world,
        OffscreenWork::SetupCamera {
            camera,
            view: Mat4::IDENTITY.to_cols_array(),
            projection: Mat4::IDENTITY.to_cols_array(),
            target,
            buffers: None,
        },
    );
}

/// What each clear of the frame binds.
fn bound(adapted: &Adapted) -> Vec<Attachments> {
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

fn whole(texture: worldrunner_render_program::ids::TextureId) -> Attachment {
    Attachment { texture, mip: 0 }
}

fn screen_bound() -> Attachments {
    Attachments {
        colors: vec![whole(ids::screen_color())],
        depth: Some(whole(ids::screen_color())),
    }
}

#[test]
fn a_target_that_names_the_camera_target_binds_the_screen_when_one_is_set() {
    let mut world = world();
    record(&mut world, bind(&[TargetRef::CameraTarget], None));
    record(&mut world, clear_all());
    let adapted = run(&mut world, Some(SIZE));
    assert_eq!(bound(&adapted), [screen_bound()]);
    assert!(
        adapted.report.skipped.is_empty(),
        "{:?}",
        adapted.report.skipped
    );
}

#[test]
fn a_depth_target_beside_the_camera_target_binds_the_screens_own_depth() {
    let mut world = world();
    record(
        &mut world,
        bind(&[TargetRef::CameraTarget], Some(TargetRef::CameraTarget)),
    );
    record(&mut world, clear_all());
    let adapted = run(&mut world, Some(SIZE));
    assert_eq!(bound(&adapted), [screen_bound()]);
}

#[test]
fn a_camera_that_has_no_texture_draws_into_the_screen() {
    let mut world = world();
    let camera = camera(&mut world);
    set_up(&mut world, camera, TargetRef::CameraTarget);
    record(&mut world, clear_all());
    let adapted = run(&mut world, Some(SIZE));
    assert_eq!(bound(&adapted), [screen_bound()]);
    assert!(
        adapted.report.skipped.is_empty(),
        "{:?}",
        adapted.report.skipped
    );
}

#[test]
fn the_camera_target_of_a_camera_with_a_texture_is_still_that_texture() {
    let mut world = world();
    let texture = render_texture(&mut world, [4, 4]);
    let camera = camera(&mut world);
    set_up(&mut world, camera, TargetRef::Render(texture));
    record(&mut world, bind(&[TargetRef::CameraTarget], None));
    record(&mut world, clear_all());
    let adapted = run(&mut world, Some(SIZE));
    let attachment = whole(ids::render_texture(texture));
    assert_eq!(
        bound(&adapted),
        [Attachments {
            colors: vec![attachment],
            depth: Some(attachment)
        }]
    );
    assert!(
        adapted
            .frame
            .render_textures
            .iter()
            .all(|spec| !ids::is_screen(spec.id)),
        "the frame does not hold the screen"
    );
}

#[test]
fn the_screen_is_described_as_one_render_texture_with_colour_and_depth() {
    let mut world = world();
    record(&mut world, bind(&[TargetRef::CameraTarget], None));
    record(&mut world, clear_all());
    let adapted = run(&mut world, Some(SIZE));
    assert_eq!(
        adapted.frame.render_textures,
        [RenderTextureSpec {
            id: ids::screen_color(),
            size: SIZE,
            dimension: RenderDimension::D2,
            graphics_format: 4,
            depth_bits: 24,
            mips: 1,
            srgb: true,
            generation: 0,
        }]
    );
}

#[test]
fn a_depth_only_binding_of_the_camera_target_binds_the_screens_own_depth() {
    let mut world = world();
    let camera = camera(&mut world);
    set_up(&mut world, camera, TargetRef::CameraTarget);
    record(&mut world, bind(&[], Some(TargetRef::CameraTarget)));
    record(&mut world, clear_all());
    let adapted = run(&mut world, Some(SIZE));
    let screen = Some(whole(ids::screen_color()));
    assert_eq!(
        bound(&adapted),
        [Attachments {
            colors: Vec::new(),
            depth: screen
        }]
    );
}

#[test]
fn a_frame_that_does_not_bind_the_screen_does_not_hold_it() {
    let mut world = world();
    let texture = render_texture(&mut world, [4, 4]);
    record(&mut world, bind(&[TargetRef::Render(texture)], None));
    record(&mut world, clear_all());
    let adapted = run(&mut world, Some(SIZE));
    assert_eq!(adapted.frame.render_textures.len(), 1);
}

#[test]
fn without_a_screen_the_camera_target_is_left_out_by_reason() {
    let mut world = world();
    record(&mut world, bind(&[TargetRef::CameraTarget], None));
    record(&mut world, clear_all());
    let adapted = run(&mut world, None);
    assert_eq!(adapted.report.skipped.get(&Skip::UnboundTarget), Some(&2));
    assert!(adapted.frame.records.is_empty());
    assert!(adapted.frame.render_textures.is_empty());
}

#[test]
fn the_screens_names_belong_to_no_other_texture() {
    let mut world = world();
    let texture = render_texture(&mut world, [4, 4]);
    let drawn = ids::render_texture(texture);
    let screen = ids::screen_color();
    assert!(ids::is_screen(screen));
    assert_ne!(screen, drawn);
    assert!(ids::render_texture_of(screen).is_none());
    assert!(!ids::is_temporary(screen));
    assert!(!ids::is_probe_scratch(screen));
    assert!(ids::stored_texture(screen.0).is_none());
    assert!(!ids::is_screen(drawn));
    assert!(!ids::is_screen(ids::temporary(7, 0)));
    assert!(!ids::is_screen(ids::probe_scratch(drawn)));
}

fn clear_colors(adapted: &Adapted) -> Vec<Option<[f64; 4]>> {
    adapted
        .frame
        .records
        .iter()
        .filter_map(|record| match record {
            Record::Clear { clear, .. } => Some(clear.color),
            _ => None,
        })
        .collect()
}

#[test]
fn a_camera_setup_restores_the_screen_after_a_partial_viewport() {
    let mut world = world();
    let camera = camera(&mut world);
    record(&mut world, bind(&[TargetRef::CameraTarget], None));
    record(
        &mut world,
        OffscreenWork::SetViewport {
            rect: [0.0, 0.0, 4.0, f32::from(u8::try_from(SIZE[1]).unwrap())],
        },
    );
    record(&mut world, clear_all());
    set_up(&mut world, camera, TargetRef::CameraTarget);
    record(&mut world, clear_with_color([1.0, 0.0, 0.0, 1.0]));

    let adapted = run(&mut world, Some(SIZE));
    assert_eq!(adapted.report.skipped.get(&Skip::PartialViewport), Some(&1));
    assert_eq!(bound(&adapted), [screen_bound()]);
    assert_eq!(clear_colors(&adapted), [Some([1.0, 0.0, 0.0, 1.0])]);
}

#[test]
fn a_camera_setup_without_a_target_clears_the_previous_binding() {
    for target in [TargetRef::None, TargetRef::CameraTarget] {
        let mut world = world();
        let texture = render_texture(&mut world, [4, 4]);
        let attachment = whole(ids::render_texture(texture));
        record(&mut world, bind(&[TargetRef::Render(texture)], None));
        record(&mut world, clear_all());
        let camera = camera(&mut world);
        set_up(&mut world, camera, target);
        record(&mut world, clear_with_color([1.0, 0.0, 0.0, 1.0]));

        let adapted = run(&mut world, None);
        assert_eq!(
            bound(&adapted),
            [Attachments {
                colors: vec![attachment],
                depth: Some(attachment),
            }]
        );
        assert_eq!(adapted.report.skipped.get(&Skip::UnboundTarget), Some(&1));
        assert_eq!(clear_colors(&adapted), [Some([0.25, 0.5, 0.75, 1.0])]);
    }
}
