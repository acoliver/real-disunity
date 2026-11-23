//! The stock terrain draws, switched off by a game that draws the terrain while the terrain stays
//! resident.
use super::declaring::declaring;
use super::fixture::{Rig, SIZE, terrain_scene};
use super::strip::{LIGHT, STRIP, StripGame, knobs};
use std::sync::Arc;
use worldrunner_render_scene::compositor::{GamePasses, StockReplacement};

fn centre(image: &[[u8; 4]]) -> [u8; 4] {
    Rig::pixel(image, SIZE[0] as usize / 2, SIZE[1] as usize / 2)
}

#[test]
fn a_declared_terrain_is_resident_but_drawn_by_no_stock_pass() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let mut stock = Rig::new(&terrain_scene(), SIZE, |_| {});
    let drawn = stock.render().unwrap();
    assert_eq!(
        centre(&drawn),
        [255; 4],
        "the stock frame draws the terrain"
    );
    assert!(stock.gpu.statistics().passes.terrain.calls > 0);

    let mut rig = Rig::new(&terrain_scene(), SIZE, |settings| {
        settings.game_passes = declaring(StockReplacement::TERRAIN_SURFACE);
    });
    let image = rig.render().unwrap();
    assert!(
        image.iter().all(|pixel| *pixel == [0, 0, 0, 255]),
        "no pixel of the terrain"
    );
    let passes = rig.gpu.statistics().passes;
    assert_eq!(
        passes.terrain.calls, 0,
        "no terrain draw in the colour pass"
    );
    assert!(
        passes
            .shadow_cascades
            .iter()
            .all(|cascade| cascade.calls == 0),
        "no shadow in this frame, so no caster draws either"
    );

    // The terrain never left: a renderer built without the declaring game shows it at once.
    let mut settings = rig.renderer.configuration().clone();
    settings.game_passes = GamePasses::new();
    rig.renderer.reconfigure(&rig.context, settings).unwrap();
    assert_eq!(centre(&rig.render().unwrap()), [255; 4]);
}

#[test]
fn no_factories_and_a_game_that_declares_nothing_leave_the_frame_as_the_stock_frame() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let reference = Rig::new(&terrain_scene(), SIZE, |_| {}).render().unwrap();
    let explicit = Rig::new(&terrain_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new();
    })
    .render()
    .unwrap();
    assert_eq!(explicit, reference);
    let silent = Rig::new(&terrain_scene(), SIZE, |settings| {
        settings.game_passes = declaring(StockReplacement::NONE);
    })
    .render()
    .unwrap();
    assert_eq!(silent, reference);
}

#[test]
fn a_game_feature_that_declares_the_terrain_draws_over_no_stock_terrain() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    // The strip covers the right half of the frame; the terrain is drawn left of it too.
    let strip = knobs([0.0, -1.0, 0.8, 1.0], 0.2);
    strip.lock().unwrap().replaces = StockReplacement::TERRAIN_SURFACE;
    let game = Arc::new(StripGame {
        knobs: strip.clone(),
    });
    let mut rig = Rig::new(&terrain_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new().with(game);
    });
    let image = rig.render().unwrap();
    assert_eq!(
        strip.lock().unwrap().counts.recorded,
        vec![STRIP, LIGHT],
        "the game's passes ran"
    );
    let pixel = |column, row| Rig::pixel(&image, column, row);
    assert_eq!(pixel(54, 24)[..2], [255, 255], "the strip is drawn");
    assert_eq!(pixel(24, 24), [0, 0, 0, 255], "no stock terrain left of it");
    assert_eq!(rig.gpu.statistics().passes.terrain.calls, 0);

    // The terrain stayed resident: rebuilding without the game shows it.
    let mut settings = rig.renderer.configuration().clone();
    settings.game_passes = GamePasses::new();
    rig.renderer.reconfigure(&rig.context, settings).unwrap();
    let image = rig.render().unwrap();
    assert_eq!(Rig::pixel(&image, 24, 24), [255; 4]);
}
