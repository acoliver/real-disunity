//! A replacement builds its targets privately and publishes them only when the whole generation
//! is built: a refused one leaves the live renderer's targets as they were, and renderers built
//! from cloned inventories never rebind each other.
use super::fixture::{Rig, SCENE_FORMAT, SIZE};
use super::layers::{ARRAY, HISTORY};
use super::persistence::{assert_filled, fill_all, rig};
use worldrunner_render_post::{FrameConfig, FrameError};
use worldrunner_render_scene::wgpu;

const LARGE: [u32; 2] = [128, 96];

/// What a renderer reports about itself that a refused replacement must leave alone.
#[derive(Debug, PartialEq, Eq)]
struct Standing {
    generation: u64,
    budget: u64,
    resident: u64,
}

impl Standing {
    fn of(rig: &Rig) -> Self {
        Self {
            generation: rig.renderer.generation(),
            budget: rig.renderer.budget().0,
            resident: rig.resident_bytes(),
        }
    }
}

fn extent(texture: &wgpu::Texture) -> [u32; 2] {
    [texture.width(), texture.height()]
}

#[test]
fn a_replacement_the_factory_refuses_leaves_the_live_renderers_targets_exactly_as_they_were() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut rig, knobs) = rig();
    fill_all(&knobs);
    rig.render().unwrap();
    let targets = rig.renderer.game_targets();
    let history = targets.texture(HISTORY).unwrap();
    let array = targets.texture(ARRAY).unwrap();
    assert_eq!(extent(&history), SIZE);
    assert!(targets.is_written(HISTORY, 0), "the sentinel was submitted");
    let standing = Standing::of(&rig);

    let mut larger = rig.renderer.configuration().clone();
    larger.frame = FrameConfig::new(LARGE, SCENE_FORMAT, 1);
    knobs.lock().unwrap().refuse_build = true;
    assert!(
        rig.renderer
            .reconfigure(&rig.context, larger.clone())
            .is_err(),
        "the factory refused the larger generation"
    );

    let targets = rig.renderer.game_targets();
    assert_eq!(
        targets.texture(HISTORY).unwrap(),
        history,
        "texture identity"
    );
    assert_eq!(extent(&targets.texture(HISTORY).unwrap()), SIZE, "extent");
    assert!(targets.is_written(HISTORY, 0), "written history");
    assert_eq!(targets.texture(ARRAY).unwrap(), array);
    assert_eq!(
        Standing::of(&rig),
        standing,
        "generation, budget and charge"
    );
    knobs.lock().unwrap().produce.clear();
    assert_filled(
        &rig.render().unwrap(),
        "the live renderer after the refusal",
    );

    knobs.lock().unwrap().refuse_build = false;
    rig.renderer.reconfigure(&rig.context, larger).unwrap();
    let targets = rig.renderer.game_targets();
    assert_eq!(rig.renderer.generation(), standing.generation + 1);
    assert_eq!(extent(&targets.texture(HISTORY).unwrap()), LARGE);
    assert_ne!(targets.texture(HISTORY).unwrap(), history, "recreated at B");
    assert!(
        !targets.is_written(HISTORY, 0),
        "a resize discards contents"
    );
    assert_eq!(
        targets.texture(ARRAY).unwrap(),
        array,
        "the fixed array stays"
    );
    assert!(targets.is_written(ARRAY, 0));
}

#[test]
fn renderers_built_from_cloned_inventories_never_share_or_rebind_each_others_targets() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut first, knobs) = rig();
    fill_all(&knobs);
    first.render().unwrap();
    let before = first.renderer.game_targets();
    let (history, array) = (
        before.texture(HISTORY).unwrap(),
        before.texture(ARRAY).unwrap(),
    );

    let mut larger = first.renderer.configuration().clone();
    larger.frame = FrameConfig::new(LARGE, SCENE_FORMAT, 1);
    let second = first.build_renderer(larger);

    let after = first.renderer.game_targets();
    assert_eq!(
        after.texture(HISTORY).unwrap(),
        history,
        "first keeps its history"
    );
    assert_eq!(extent(&after.texture(HISTORY).unwrap()), SIZE);
    assert!(after.is_written(HISTORY, 0));
    assert_eq!(after.texture(ARRAY).unwrap(), array);
    let theirs = second.game_targets();
    assert_ne!(
        theirs.texture(ARRAY).unwrap(),
        array,
        "second owns its array"
    );
    assert_ne!(theirs.texture(HISTORY).unwrap(), history);
    assert!(
        !theirs.is_written(ARRAY, 0),
        "nothing of first's writes leaks"
    );
    knobs.lock().unwrap().produce.clear();
    assert_filled(&first.render().unwrap(), "first after second was built");
}

#[test]
fn a_second_renderer_that_nothing_has_written_refuses_the_read_by_name() {
    let _session = worldrunner_render_scene::test_support::SceneTestSession::acquire();
    let (mut first, knobs) = rig();
    fill_all(&knobs);
    first.render().unwrap();
    let settings = first.renderer.configuration().clone();
    let mut second = Rig::new(&super::fixture::quad_scene(), SIZE, |configured| {
        *configured = settings;
    });
    knobs.lock().unwrap().produce.clear();
    let Err(error) = second.render() else {
        panic!("the second renderer's array was never written");
    };
    assert!(
        matches!(error, FrameError::MissingProducer(resource) if resource.name == ARRAY),
        "{error:?}"
    );
}
