//! The game's screen composited in place of the stock scene: from the operations that write it in
//! the same frame, from contents an earlier submission left, and not at all when neither exists.
//!
//! The screen is the camera's 8 by 8 render texture stored as sRGB, red over its first four
//! columns, the camera's yellow clear over the next two and blue over the last two. Each texel is
//! eight pixels of the 64 pixel output, so every expected pixel is derived from those columns.
//! The stock scene would show a black clear and its red card, which none of them match.

use super::super::rig::Image;
use super::*;
use worldrunner_render_post::{AttachmentView, FrameError};

use crate::compositor::{GameScreenFrame, GameScreenPass};

const SCREEN: ResourceRef = ResourceRef::new("game-screen");

fn fixture() -> Fixture {
    Fixture::new(settings(), |device| {
        ScenePipelines::new(device, SCENE_FORMAT, 1).unwrap()
    })
}

/// The camera's frame without its blit and copy, drawn into a texture stored as sRGB.
fn screen_frame() -> OffscreenFrame {
    let mut screen = frame();
    screen.render_textures = vec![RenderTextureSpec {
        graphics_format: 4,
        srgb: true,
        ..texture(A, 1)
    }];
    screen.records.truncate(1);
    screen.later_reads.clear();
    screen
}

/// The operations that draw the screen, exporting it under the game screen's name, and the
/// texture they write.
fn screen_operations(player: &mut Player) -> (OffscreenOperations, wgpu::Texture) {
    let mut operations = player
        .state
        .build(&mut player.runtime, &player.arena, &screen_frame())
        .unwrap();
    operations.export(A, 0, 0, SCREEN).unwrap();
    let handle = player.state.texture_handle(A).unwrap();
    (operations, player.runtime.gpu_texture(handle).unwrap())
}

fn pass(fixture: &mut Fixture) -> GameScreenPass {
    let device = fixture.context.device().clone();
    fixture
        .renderer
        .encoding()
        .game_screen_pass(&device, SCREEN)
        .unwrap()
}

/// Records one frame that shows `screen`, with `operations` admitted ahead of it when given,
/// submits it and reads the output. A refused frame is cancelled and gives its error.
fn show(
    fixture: &mut Fixture,
    shown: (&mut GameScreenPass, &wgpu::Texture, bool),
    operations: Option<OffscreenOperations>,
) -> Result<Image, FrameError> {
    let authority = fixture.context.submission().clone();
    let device = fixture.context.device().clone();
    let mut recording = authority.begin(&device).unwrap();
    if let Some(operations) = operations {
        fixture
            .gpu
            .admit_offscreen(operations.admit(&recording).unwrap());
    }
    let color = AttachmentView::new(
        ResourceRef::new("scene color"),
        fixture.renderer.targets().scene().texture(),
    )
    .unwrap();
    let (pass, screen, retained) = shown;
    let encoded = fixture.renderer.encoding().encode_game_screen(
        &device,
        &mut recording,
        &mut fixture.gpu,
        GameScreenFrame {
            pass,
            screen,
            retained,
            color: &color,
        },
    );
    if let Err(error) = encoded {
        recording.finish().cancel();
        return Err(error);
    }
    let output = fixture.output_copy(&mut recording);
    recording.finish().submit().unwrap();
    Ok(fixture.read(&output))
}

fn assert_screen(image: &Image) {
    assert_eq!(image.pixel(4, 32), RED);
    assert_eq!(
        image.pixel(36, 32),
        YELLOW,
        "the stock scene's red card is covered"
    );
    assert_eq!(image.pixel(52, 32), BLUE, "the stock black is covered");
    assert_eq!(image.pixel(3, 3), RED);
    assert_eq!(image.pixel(60, 60), BLUE);
}

#[test]
fn operations_that_draw_the_screen_replace_the_stock_scene_in_their_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let mut player = player(&mut fixture);
    let (operations, screen) = screen_operations(&mut player);
    let mut pass = pass(&mut fixture);
    let before = fixture.context.submission().submitted_serial();

    let image = show(&mut fixture, (&mut pass, &screen, false), Some(operations)).unwrap();

    assert_screen(&image);
    assert_eq!(
        fixture.context.submission().submitted_serial(),
        before + 1,
        "the screen's writer, its composite and the output are one submission"
    );
}

#[test]
fn a_screen_an_earlier_frame_wrote_is_shown_by_a_frame_with_no_writer() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let mut player = player(&mut fixture);
    let (operations, screen) = screen_operations(&mut player);
    let mut pass = pass(&mut fixture);
    show(&mut fixture, (&mut pass, &screen, false), Some(operations)).unwrap();

    let later = show(&mut fixture, (&mut pass, &screen, true), None).unwrap();

    assert_screen(&later);
}

#[test]
fn a_frame_with_neither_a_writer_nor_earlier_contents_is_refused_and_submits_nothing() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let mut player = player(&mut fixture);
    // Built and never admitted: the frame that was to write the screen was dropped.
    let (_dropped, screen) = screen_operations(&mut player);
    let mut pass = pass(&mut fixture);
    let before = fixture.context.submission().submitted_serial();

    let refused = show(&mut fixture, (&mut pass, &screen, false), None);

    assert_eq!(
        refused.map(|_| ()),
        Err(FrameError::MissingProducer(SCREEN))
    );
    assert_eq!(fixture.context.submission().submitted_serial(), before);
}

#[test]
fn a_frame_that_shows_the_screen_refuses_player_program_draws_by_name() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let mut player = player(&mut fixture);
    let (operations, screen) = screen_operations(&mut player);
    let mut pass = pass(&mut fixture);
    let authority = fixture.context.submission().clone();
    let recording = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    let programs = fixture.admit(&recording, signature).unwrap();
    fixture.gpu.admit_programs(programs);
    recording.finish().cancel();

    let refused = show(&mut fixture, (&mut pass, &screen, false), Some(operations));

    assert_eq!(
        refused.map(|_| ()),
        Err(FrameError::UnsupportedEffect(
            "player program draws with the game screen"
        ))
    );
}

#[test]
fn a_refused_screen_frame_does_not_leave_its_writer_for_the_next_frame() {
    let _session = crate::test_support::SceneTestSession::acquire();
    let mut fixture = fixture();
    let mut player = player(&mut fixture);
    let (operations, screen) = screen_operations(&mut player);
    let mut pass = pass(&mut fixture);
    let authority = fixture.context.submission().clone();
    let recording = authority.begin(fixture.context.device()).unwrap();
    let signature = fixture.signature();
    let programs = fixture.admit(&recording, signature).unwrap();
    fixture.gpu.admit_programs(programs);
    recording.finish().cancel();
    let before = authority.submitted_serial();

    // The frame supplies a valid writer of the screen and is refused before its operations are
    // taken, by the program draws the screen frame cannot show.
    let refused = show(&mut fixture, (&mut pass, &screen, false), Some(operations));
    assert_eq!(
        refused.map(|_| ()),
        Err(FrameError::UnsupportedEffect(
            "player program draws with the game screen"
        ))
    );
    assert_eq!(
        authority.submitted_serial(),
        before,
        "a refused frame submits nothing"
    );

    // The next frame brings no writer and nothing wrote the screen. The refused frame's writer
    // must not run in it: with it, the frame would show the screen the writer draws.
    let next = show(&mut fixture, (&mut pass, &screen, false), None);
    assert_eq!(
        next.map(|_| ()),
        Err(FrameError::MissingProducer(SCREEN)),
        "the refused frame's writer ran in a frame that supplied none"
    );
    assert_eq!(authority.submitted_serial(), before);

    // Reading the screen back through a frame that shows it as retained contents finds the
    // texture as nobody wrote it.
    let unwritten = show(&mut fixture, (&mut pass, &screen, true), None).unwrap();
    assert_ne!(
        unwritten.pixel(4, 32),
        RED,
        "the abandoned writer drew the screen"
    );
    assert_ne!(unwritten.pixel(36, 32), YELLOW);
    assert_ne!(unwritten.pixel(52, 32), BLUE);
}
