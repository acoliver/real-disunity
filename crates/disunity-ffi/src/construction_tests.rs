use crate::{
    abi::*, construction::*, hierarchy::disunity_transform_parent, scheduling::*,
    sprites::disunity_sprite_create, store::lock_store,
};

const AWAKE: u32 = 0;
const ON_ENABLE: u32 = 1;

struct Built {
    construction: u64,
    root: u64,
    child: u64,
}

unsafe fn object(world: u64, construction: u64) -> u64 {
    let mut handle = 0;
    let result = unsafe { disunity_construction_object_create(world, construction, &mut handle) };
    assert_eq!(result.code, 0);
    handle
}

unsafe fn script(world: u64, object: u64, identity: u64) {
    let mut handle = 0;
    let result = unsafe { disunity_script_register(world, object, identity, &mut handle) };
    assert_eq!(result.code, 0);
}

/// A root with a sprite and script 51, and a child with script 52.
unsafe fn build(world: u64) -> Built {
    unsafe {
        let mut construction = 0;
        assert_eq!(
            disunity_construction_begin(world, &mut construction).code,
            0
        );
        let root = object(world, construction);
        let child = object(world, construction);
        let mut work = DisunityAdvance::default();
        let parented = disunity_transform_parent(world, child, root, 0, &mut work);
        assert_eq!(
            (parented.code, work.kind),
            (0, 2),
            "no lifecycle work while constructing"
        );
        let mut sprite = 0;
        assert_eq!(disunity_sprite_create(world, root, &mut sprite).code, 0);
        script(world, root, 51);
        script(world, child, 52);
        Built {
            construction,
            root,
            child,
        }
    }
}

/// Resumes every callback, returning (identity, message) for each.
unsafe fn drain(world: u64, work: &mut DisunityAdvance) -> Vec<(u64, u32)> {
    let mut calls = Vec::new();
    while work.kind == 1 {
        calls.push((work.script, work.message));
        let result =
            unsafe { disunity_world_resume(world, work.ticket, 0, std::ptr::null(), 0, work) };
        assert_eq!(result.code, 0);
    }
    assert_eq!(work.kind, 2);
    calls
}

fn world() -> u64 {
    let mut world = 0;
    assert_eq!(unsafe { disunity_world_create(&mut world) }.code, 0);
    world
}

/// Session state copied out under the store lock. Tests assert on the copy, so a failing
/// assertion cannot poison the global lock for other tests.
#[derive(Debug, PartialEq)]
struct State {
    storage: (usize, usize),
    published: usize,
    objects: Vec<u64>,
    scripts: usize,
    components: usize,
    constructions: usize,
}

fn state(world: u64) -> State {
    let store = lock_store();
    let session = store.session(world).unwrap();
    State {
        storage: session.native.storage_counts(),
        published: session.native.snapshot().objects.len(),
        objects: session.objects.iter().map(|(&id, _)| id).collect(),
        scripts: session.dispatch.scripts.len(),
        components: session.dispatch.components.len(),
        constructions: session.constructions.len(),
    }
}

#[test]
fn activation_runs_every_awake_before_any_enable_and_closes_the_construction() {
    unsafe {
        let world = world();
        let built = build(world);
        assert_eq!(state(world).published, 0);
        let mut work = DisunityAdvance::default();
        let result = disunity_construction_activate(world, built.construction, &mut work);
        assert_eq!(result.code, 0);
        assert_eq!(
            drain(world, &mut work),
            [(51, AWAKE), (52, AWAKE), (51, ON_ENABLE), (52, ON_ENABLE)]
        );
        assert_eq!(state(world).published, 2);
        assert_eq!(state(world).constructions, 0);
        let again = disunity_construction_activate(world, built.construction, &mut work);
        assert_ne!(again.code, 0, "an activated construction is closed");
        assert_ne!(
            disunity_construction_abort(world, built.construction).code,
            0
        );
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

#[test]
fn abort_releases_objects_components_and_script_identities() {
    unsafe {
        let world = world();
        let mut existing = 0;
        assert_eq!(disunity_gameobject_create(world, &mut existing).code, 0);
        let before = state(world).storage;
        let built = build(world);
        assert_eq!(
            disunity_construction_abort(world, built.construction).code,
            0
        );
        let after = state(world);
        assert_eq!(after.storage, before);
        assert_eq!(after.objects, [existing]);
        assert_eq!(
            (after.scripts, after.components, after.constructions),
            (0, 0, 0)
        );
        for handle in [built.root, built.child] {
            let mut sprite = 0;
            assert_ne!(disunity_sprite_create(world, handle, &mut sprite).code, 0);
        }
        let mut handle = 0;
        let late = disunity_construction_object_create(world, built.construction, &mut handle);
        assert_ne!(late.code, 0, "an aborted construction is closed");
        let rebuilt = build(world);
        let mut work = DisunityAdvance::default();
        assert_eq!(
            disunity_construction_activate(world, rebuilt.construction, &mut work).code,
            0
        );
        assert_eq!(
            drain(world, &mut work).len(),
            4,
            "identities 51 and 52 are reusable"
        );
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

#[test]
fn constructions_belong_to_their_world_and_reject_null_outputs() {
    unsafe {
        let (first, second) = (world(), world());
        let built = build(first);
        let mut handle = 0;
        let foreign = disunity_construction_object_create(second, built.construction, &mut handle);
        assert_ne!(foreign.code, 0);
        assert_ne!(
            disunity_construction_abort(second, built.construction).code,
            0
        );
        let null = std::ptr::null_mut();
        assert_ne!(disunity_construction_begin(first, null).code, 0);
        assert_ne!(
            disunity_construction_object_create(first, built.construction, null).code,
            0
        );
        assert_ne!(
            disunity_construction_activate(first, built.construction, std::ptr::null_mut()).code,
            0
        );
        assert_eq!(
            disunity_construction_abort(first, built.construction).code,
            0
        );
        assert_eq!(disunity_world_destroy(first).code, 0);
        assert_eq!(disunity_world_destroy(second).code, 0);
    }
}
