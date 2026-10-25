use worldrunner_world_simulation::{
    Advance, CallbackOutcome, ScriptMessage, TimeSpan, World, WorldSettings,
};

fn drain(world: &mut World, mut work: Advance) -> Vec<(u64, ScriptMessage)> {
    let mut calls = Vec::new();
    while let Advance::AssetCallback(callback) = work {
        calls.push((callback.identity, callback.message));
        work = world
            .resume(callback.ticket, CallbackOutcome::Returned)
            .unwrap();
    }
    calls
}
#[test]
fn standalone_asset_lifetime_does_not_allocate_scene_storage() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.create_script_asset(1).unwrap();
    assert!(world.create_script_asset(1).is_err());
    assert_eq!(world.storage_counts(), (0, 0));
    let work = world.activate_script_asset(1).unwrap();
    assert_eq!(
        drain(&mut world, work),
        [(1, ScriptMessage::Awake), (1, ScriptMessage::OnEnable)]
    );
    world.destroy_script_asset(1).unwrap();
    world.destroy_script_asset(1).unwrap();
    let work = world.advance(TimeSpan::ZERO).unwrap();
    assert_eq!(
        drain(&mut world, work),
        [(1, ScriptMessage::OnDisable), (1, ScriptMessage::OnDestroy)]
    );
    assert!(!world.script_asset_alive(1));
}
#[test]
fn nested_asset_creation_preserves_the_parent_ticket() {
    let mut world = World::new(WorldSettings::default()).unwrap();
    world.create_script_asset(1).unwrap();
    let Advance::AssetCallback(parent) = world.activate_script_asset(1).unwrap() else {
        panic!()
    };
    world.create_script_asset(2).unwrap();
    let child = world.activate_script_asset(2).unwrap();
    assert_eq!(drain(&mut world, child).len(), 2);
    let parent = world
        .resume(parent.ticket, CallbackOutcome::Returned)
        .unwrap();
    assert_eq!(drain(&mut world, parent), [(1, ScriptMessage::OnEnable)]);
}
