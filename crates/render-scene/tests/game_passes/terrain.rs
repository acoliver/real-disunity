//! The stock terrain draws, switched off while the terrain stays resident.
use super::fixture::{Rig, SIZE, terrain_scene};
use worldrunner_render_scene::compositor::TerrainDraws;

fn centre(image: &[[u8; 4]]) -> [u8; 4] {
    Rig::pixel(image, SIZE[0] as usize / 2, SIZE[1] as usize / 2)
}

#[test]
fn suppressed_terrain_is_resident_but_drawn_by_no_stock_pass() {
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
        settings.terrain_draws = TerrainDraws::Suppressed;
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
            .all(|cascade| cascade.calls == 0)
    );

    // The terrain never left: selecting the stock draws again shows it at once.
    let mut settings = rig.renderer.configuration().clone();
    settings.terrain_draws = TerrainDraws::Stock;
    rig.renderer.reconfigure(&rig.context, settings).unwrap();
    assert_eq!(centre(&rig.render().unwrap()), [255; 4]);
}

#[test]
fn no_factories_and_stock_draws_leave_the_frame_as_the_stock_frame() {
    use worldrunner_render_scene::compositor::GamePasses;
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let reference = Rig::new(&terrain_scene(), SIZE, |_| {}).render().unwrap();
    let explicit = Rig::new(&terrain_scene(), SIZE, |settings| {
        settings.game_passes = GamePasses::new();
        settings.terrain_draws = TerrainDraws::Stock;
    })
    .render()
    .unwrap();
    assert_eq!(explicit, reference);
}
