use crate::{abi::*, cloning::*, scheduling::*, store::lock_store};

unsafe fn scene(world: u64, identity: u64) -> u64 {
    let (mut object, mut script) = (0, 0);
    unsafe {
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        if identity != 0 {
            assert_eq!(
                disunity_script_register(world, object, identity, &mut script).code,
                0
            );
        }
    }
    object
}

unsafe fn finish(world: u64, work: &mut DisunityAdvance) -> DisunityResult {
    let mut result = DisunityResult::ok();
    while work.kind == 1 && result.code == 0 {
        assert!(matches!(work.script, 41 | 42));
        result = unsafe { disunity_world_resume(world, work.ticket, 0, std::ptr::null(), 0, work) };
    }
    result
}

fn assert_teardown(world: u64, reservations: usize) {
    let store = lock_store();
    let session = store.session(world).unwrap();
    assert_eq!(session.native.storage_counts(), (0, 0));
    assert_eq!(session.clones.len(), reservations);
    assert_eq!(session.objects.len(), 0);
    assert!(session.dispatch.tickets.is_empty());
    assert!(session.dispatch.payloads.is_empty());
    assert!(session.dispatch.waits.is_empty());
}

#[test]
fn failed_nested_awake_releases_unbound_reservation_after_native_teardown() {
    unsafe {
        let mut world = 0;
        assert_eq!(disunity_world_create(&mut world).code, 0);
        let template = scene(world, 41);
        let parent = scene(world, 0);
        let mut work = DisunityAdvance::default();
        assert_eq!(disunity_object_wake(world, template, &mut work).code, 0);
        assert_eq!(finish(world, &mut work).code, 0);
        let (mut root, mut count) = (0, 0);
        assert_eq!(
            disunity_clone_create(world, template, parent, &mut root, &mut count).code,
            0
        );
        let nested = scene(world, 42);
        assert_eq!(disunity_object_wake(world, nested, &mut work).code, 0);
        let failure = b"original nested Awake failure";
        let result = disunity_world_resume(
            world,
            work.ticket,
            1,
            failure.as_ptr(),
            failure.len() as u32,
            &mut work,
        );
        assert_eq!(result.code, 0);
        assert_ne!(finish(world, &mut work).code, 0);
        assert_teardown(world, 1);
        assert_eq!(disunity_clone_abort(world, root).code, 0);
        assert_teardown(world, 0);
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}

/// A template's natively bound component gets a component row for its copy, naming the
/// template's identity, and the copy is bound under the new identity.
#[test]
fn a_bound_native_component_is_copied_with_a_component_row() {
    unsafe {
        let mut world = 0;
        assert_eq!(disunity_world_create(&mut world).code, 0);
        let template = scene(world, 0);
        let source = 5_000_001;
        {
            let mut store = lock_store();
            let session = store.session_mut(world).unwrap();
            let handle = session.object(template).unwrap();
            let transform = session.native.object(handle).unwrap().transform;
            session.dispatch.components.insert(source, transform);
        }
        let (mut root, mut count) = (0, 0);
        assert_eq!(
            disunity_clone_create(world, template, 0, &mut root, &mut count).code,
            0
        );
        let mut rows = vec![DisunityCloneRow::default(); count as usize];
        let mut written = 0;
        let copied = disunity_clone_copy(world, root, rows.as_mut_ptr(), count, &mut written);
        assert_eq!(copied.code, 0);
        let components: Vec<_> = rows.iter().filter(|row| row.kind == 2).collect();
        assert_eq!(components.len(), 1, "{rows:?}");
        assert_eq!(components[0].source, source);
        let target = components[0].target;
        let store = lock_store();
        let session = store.session(world).unwrap();
        let copy = *session.dispatch.components.get(&target).unwrap();
        assert_eq!(session.dispatch.components.identity_of(copy), Some(target));
    }
}
