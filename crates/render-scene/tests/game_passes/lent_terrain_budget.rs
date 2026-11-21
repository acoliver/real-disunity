//! The terrain material arrays the engine lends are charged to the same budget and runtime ledger
//! as the factory's declared resources, and the charge is returned when the array is dropped.
use super::fixture::{Rig, SIZE};
use super::lent_terrain_game::{
    Ask, CHANNELS, Clear, Factory, KEPT, OUT, Run, Script, Seen, layered_scene, run_with_budget,
};
use std::sync::{Arc, Mutex};
use worldrunner_render_plan::RenderScene;
use worldrunner_render_scene::{
    compositor::{
        GameBufferDeclaration, GameBufferUsage, GamePasses, GameResourceError,
        TerrainMaterialArrayError, TerrainMaterialChannel,
    },
    test_support::SceneTestSession,
};

const LABEL: &str = "engine terrain material array";
/// Four layers of 2 by 2 texels of four bytes with a 1 by 1 mip of four bytes more.
const ALBEDO_BYTES: u64 = 4 * (16 + 4);
/// The same four layers in one byte a texel.
const HEIGHT_BYTES: u64 = 4 * (4 + 1);
const ALBEDO: TerrainMaterialChannel = CHANNELS[0];
const HEIGHT: TerrainMaterialChannel = CHANNELS[2];
const BUDGET: u64 = 1 << 20;

/// What the runtime ledger of factory `factory` holds.
fn held(factory: usize) -> u64 {
    KEPT.with_borrow(|kept| kept.resources[factory].runtime().live_bytes())
}

/// Asks the game of `run` for one array and returns the answer.
fn ask(
    run: &mut Run,
    channel: TerrainMaterialChannel,
    layers: &[u8],
) -> Result<(), TerrainMaterialArrayError> {
    run.script.lock().unwrap().requests = vec![(channel, layers.to_vec())];
    run.rig.render().unwrap();
    run.seen.lock().unwrap().refusals.pop().unwrap()
}

fn budget_error(bytes: u64, held: u64, budget: u64) -> TerrainMaterialArrayError {
    TerrainMaterialArrayError::Budget(GameResourceError::Budget {
        label: LABEL.to_owned(),
        bytes,
        held,
        budget,
    })
}

#[test]
fn an_array_beyond_the_remaining_budget_is_refused_and_nothing_stays_held() {
    let _session = SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run_with_budget(&layered_scene(0), Ask::Refusals, ALBEDO_BYTES - 1);
    let (ledger, census) = (held(0), run.texture_bytes());

    assert_eq!(
        ask(&mut run, ALBEDO, &[0, 1, 2, 3]),
        Err(budget_error(ALBEDO_BYTES, ledger, ALBEDO_BYTES - 1))
    );
    assert_eq!(held(0), ledger, "a refused array holds nothing");
    assert_eq!(run.texture_bytes(), census, "a refused array is never made");

    assert_eq!(
        ask(&mut run, HEIGHT, &[0, 1, 2, 3]),
        Ok(()),
        "a smaller array that fits is still lent"
    );
}

#[test]
fn building_an_array_holds_exactly_its_physical_bytes() {
    let _session = SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run_with_budget(&layered_scene(0), Ask::Refusals, BUDGET);
    let (ledger, census) = (held(0), run.texture_bytes());

    assert_eq!(ask(&mut run, ALBEDO, &[0, 1, 2, 3]), Ok(()));
    assert_eq!(held(0), ledger + ALBEDO_BYTES);
    assert_eq!(run.texture_bytes(), census + ALBEDO_BYTES, "as the census");

    assert_eq!(ask(&mut run, ALBEDO, &[0, 1, 2, 3]), Ok(()));
    assert_eq!(
        held(0),
        ledger + ALBEDO_BYTES,
        "a cached array is held once"
    );

    assert_eq!(ask(&mut run, HEIGHT, &[0, 1, 2, 3]), Ok(()));
    assert_eq!(held(0), ledger + ALBEDO_BYTES + HEIGHT_BYTES);
}

#[test]
fn dropping_an_array_returns_its_bytes() {
    let _session = SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run_with_budget(&layered_scene(0), Ask::Refusals, BUDGET);
    let ledger = held(0);
    assert_eq!(ask(&mut run, ALBEDO, &[0, 1, 2, 3]), Ok(()));
    assert_eq!(ask(&mut run, HEIGHT, &[0, 1, 2, 3]), Ok(()));
    assert_eq!(held(0), ledger + ALBEDO_BYTES + HEIGHT_BYTES);

    run.append_texture();
    run.rig.render().unwrap();
    assert_eq!(held(0), ledger, "an appended texture drops the arrays");

    assert_eq!(ask(&mut run, ALBEDO, &[0, 1, 2, 3]), Ok(()));
    assert_eq!(held(0), ledger + ALBEDO_BYTES);
    run.replace_scene(&layered_scene(2));
    run.rig.render().unwrap();
    assert_eq!(held(0), ledger, "another scene drops the arrays");
}

#[test]
fn declared_resources_and_arrays_share_one_budget() {
    let _session = SceneTestSession::acquire();
    let _clear = Clear;
    let mut run = run_with_budget(&layered_scene(0), Ask::Refusals, BUDGET);
    let ledger = held(0);
    let room = HEIGHT_BYTES * 2;
    let filler = vec![0u8; usize::try_from(BUDGET - ledger - room).unwrap()];
    KEPT.with_borrow(|kept| {
        kept.resources[0]
            .declare_buffer(&GameBufferDeclaration {
                label: "filler",
                usage: GameBufferUsage::Storage,
                contents: &filler,
            })
            .unwrap();
    });
    assert_eq!(held(0), BUDGET - room);

    assert_eq!(
        ask(&mut run, ALBEDO, &[0, 1, 2, 3]),
        Err(budget_error(ALBEDO_BYTES, BUDGET - room, BUDGET)),
        "the declared resource leaves too little for the array"
    );
    assert_eq!(ask(&mut run, HEIGHT, &[0, 1, 2, 3]), Ok(()));
    assert_eq!(held(0), BUDGET - room + HEIGHT_BYTES);

    let refused = KEPT.with_borrow(|kept| {
        kept.resources[0].declare_buffer(&GameBufferDeclaration {
            label: "late",
            usage: GameBufferUsage::Storage,
            contents: &vec![0u8; usize::try_from(HEIGHT_BYTES).unwrap() + 4],
        })
    });
    assert_eq!(
        refused.unwrap_err(),
        GameResourceError::Budget {
            label: "late".to_owned(),
            bytes: HEIGHT_BYTES + 4,
            held: BUDGET - room + HEIGHT_BYTES,
            budget: BUDGET,
        },
        "the array's bytes count against what a game declares after it"
    );
}

/// The renderer of two factories, the second owning no target, each with its own script and
/// record of what it was answered.
struct Pair {
    run: Run,
    second: Arc<Mutex<Script>>,
    second_seen: Arc<Mutex<Seen>>,
}

fn two_factories(scene: &RenderScene, budgets: [u64; 2]) -> Pair {
    let (script, seen): (Arc<Mutex<Script>>, Arc<Mutex<Seen>>) = (Arc::default(), Arc::default());
    let (second, second_seen): (Arc<Mutex<Script>>, Arc<Mutex<Seen>>) =
        (Arc::default(), Arc::default());
    let first = Factory {
        name: OUT,
        owns_target: true,
        budget: budgets[0],
        ask: Ask::Refusals,
        script: script.clone(),
        seen: seen.clone(),
    };
    let other = Factory {
        name: "lent.second",
        owns_target: false,
        budget: budgets[1],
        ask: Ask::Refusals,
        script: second.clone(),
        seen: second_seen.clone(),
    };
    let rig = Rig::new(scene, SIZE, |settings| {
        settings.game_passes = GamePasses::new()
            .with(Arc::new(first))
            .with(Arc::new(other));
    });
    Pair {
        run: Run { rig, script, seen },
        second,
        second_seen,
    }
}

impl Pair {
    /// Has the first factory, then the second, ask for the albedo array in a frame each.
    fn both_ask(&mut self) -> Result<(), TerrainMaterialArrayError> {
        let request = vec![(ALBEDO, vec![0, 1, 2, 3])];
        self.run
            .script
            .lock()
            .unwrap()
            .requests
            .clone_from(&request);
        self.run.rig.render().unwrap();
        self.second.lock().unwrap().requests = request;
        self.run.rig.render().unwrap();
        self.second_seen.lock().unwrap().refusals.pop().unwrap()
    }
}

#[test]
fn a_factory_that_asks_for_an_array_another_made_is_charged_in_its_own_ledger() {
    let _session = SceneTestSession::acquire();
    let _clear = Clear;
    let mut pair = two_factories(&layered_scene(0), [BUDGET, BUDGET]);
    let ledgers = [held(0), held(1)];
    let census = pair.run.texture_bytes();

    assert_eq!(pair.both_ask(), Ok(()));
    assert_eq!(held(0), ledgers[0] + ALBEDO_BYTES);
    assert_eq!(held(1), ledgers[1] + ALBEDO_BYTES);
    assert_eq!(pair.run.texture_bytes(), census + ALBEDO_BYTES, "one array");

    pair.run.replace_scene(&layered_scene(1));
    pair.run.rig.render().unwrap();
    assert_eq!(
        [held(0), held(1)],
        ledgers,
        "both charges left with the array"
    );
}

#[test]
fn a_factory_whose_budget_cannot_hold_a_shared_array_is_refused_it() {
    let _session = SceneTestSession::acquire();
    let _clear = Clear;
    let mut pair = two_factories(&layered_scene(0), [BUDGET, ALBEDO_BYTES - 1]);
    let ledgers = [held(0), held(1)];

    assert_eq!(
        pair.both_ask(),
        Err(budget_error(ALBEDO_BYTES, ledgers[1], ALBEDO_BYTES - 1))
    );
    assert_eq!(held(0), ledgers[0] + ALBEDO_BYTES, "the array stays lent");
    assert_eq!(held(1), ledgers[1], "the refused factory holds nothing");
}
