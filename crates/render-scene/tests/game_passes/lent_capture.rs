//! A game that first asks the engine for the split-sum BRDF table or a terrain material array in
//! a frame that a same-frame capture fits the shadows of. A renderer that was just built or
//! reconfigured has asked for neither yet, so a screenshot taken right after is such a frame. The
//! capture admits only declared writes, and the engine's copies and integration of those items
//! are not among them, so the first frame must fill them without publishing anything the capture
//! would refuse.
use super::fixture::{Rig, SCENE_FORMAT, SIZE, quad_scene};
use super::lent::{ClearKept, KEPT, Scenario, inventory, output};
use super::lent_terrain_game::{
    Ask, Clear, Factory, OUT, Script, Seen, assert_bands, last_arrays, layered_scene,
};
use super::shadowed::{capture, effects, fitted, shadowed};
use std::sync::{Arc, Mutex};
use worldrunner_render_scene::compositor::GamePasses;

#[test]
fn a_game_that_first_asks_for_the_brdf_table_in_a_capture_is_lent_it() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = ClearKept;
    let (passes, _) = inventory(Scenario::Constants);
    let mut plain = Rig::new(&quad_scene(), SIZE, |settings| {
        settings.game_passes = passes;
    });
    plain.render().unwrap();
    let reference = output(&plain);
    let table = KEPT.with_borrow(|kept| kept.brdf.clone().expect("the table was lent"));
    let integrated = plain.read_half(&table);
    drop(plain);

    let (passes, _) = inventory(Scenario::Constants);
    let (mut rig, mut scene) = shadowed(passes);
    for frame in 0..3 {
        capture(&mut rig, &mut scene, frame);
        assert_eq!(
            output(&rig),
            reference,
            "frame {frame} draws what a plain frame draws"
        );
        let lent = KEPT.with_borrow(|kept| kept.brdf.clone().expect("the table was lent"));
        assert_eq!(
            rig.read_half(&lent),
            integrated,
            "frame {frame} reads the integrated table"
        );
    }
}

fn terrain_passes() -> (GamePasses, Arc<Mutex<Script>>, Arc<Mutex<Seen>>) {
    let script = Arc::new(Mutex::new(Script {
        layers: vec![0, 1, 2, 3],
        ..Script::default()
    }));
    let seen = Arc::new(Mutex::new(Seen::default()));
    let factory = Factory {
        name: OUT,
        owns_target: true,
        budget: 1 << 20,
        ask: Ask::Arrays,
        script: script.clone(),
        seen: seen.clone(),
    };
    (GamePasses::new().with(Arc::new(factory)), script, seen)
}

#[test]
fn a_game_that_first_asks_for_a_terrain_material_array_in_a_capture_is_lent_it() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let _clear = Clear;
    let (passes, _, _) = terrain_passes();
    let mut plain = Rig::new(&layered_scene(0), SIZE, |settings| {
        settings.game_passes = passes;
    });
    plain.render().unwrap();
    let reference = plain.read(&plain.renderer.game_targets().texture(OUT).unwrap());
    assert_bands(&reference, &[0, 1, 2, 3], 0);
    drop(plain);

    let (passes, _, seen) = terrain_passes();
    let mut rig = Rig::new(&layered_scene(0), SIZE, |settings| {
        settings.game_passes = passes;
    });
    let source = layered_scene(0);
    let lit = source
        .with_objects_and_effects(source.geometry().objects().to_vec(), Some(effects()))
        .unwrap();
    let mut scene = fitted(&rig, &lit, SCENE_FORMAT);
    let mut arrays = Vec::new();
    for frame in 0..3 {
        capture(&mut rig, &mut scene, frame);
        let image = rig.read(&rig.renderer.game_targets().texture(OUT).unwrap());
        assert_eq!(
            image, reference,
            "frame {frame} draws what a plain frame draws"
        );
        arrays.push(last_arrays());
    }
    assert!(
        seen.lock().unwrap().arrays.is_empty(),
        "no request was refused"
    );
    assert_eq!(arrays[0], arrays[1], "the array is made once");
    assert_eq!(arrays[1], arrays[2], "the array is made once");
}
