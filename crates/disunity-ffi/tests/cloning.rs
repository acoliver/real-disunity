use worldrunner_disunity_ffi::{abi::*, cloning::*, construction::*, scheduling::*};

fn template() -> (u64, u64) {
    unsafe {
        let mut world = 0;
        let mut object = 0;
        let mut script = 0;
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
        assert_eq!(
            disunity_script_register(world, object, 19, &mut script).code,
            0
        );
        (world, object)
    }
}

fn checked_rows(world: u64, root: u64, count: u32) -> [DisunityCloneRow; 2] {
    unsafe {
        assert_eq!(count, 2);
        let mut rows = [DisunityCloneRow {
            source: 99,
            ..Default::default()
        }; 2];
        let mut written = 0;
        assert_eq!(
            disunity_clone_copy(world, root, rows.as_mut_ptr(), 1, &mut written).code,
            2
        );
        assert_eq!(written, count);
        assert_eq!(rows[0].source, 99);
        assert_eq!(
            disunity_clone_copy(world, root, rows.as_mut_ptr(), 2, &mut written).code,
            0
        );
        rows
    }
}

fn bind_and_activate(world: u64, root: u64, rows: &[DisunityCloneRow]) {
    unsafe {
        let mut work = DisunityAdvance::default();
        assert_ne!(disunity_clone_activate(world, root, &mut work).code, 0);
        let row = rows.iter().find(|row| row.kind == 1).unwrap();
        assert_ne!(disunity_clone_bind(world, root, row.target, 19).code, 0);
        assert_eq!(disunity_clone_bind(world, root, row.target, 20).code, 0);
        assert_eq!(disunity_clone_activate(world, root, &mut work).code, 0);
        assert_eq!(work.kind, 1);
        while work.kind == 1 {
            assert_eq!(
                disunity_world_resume(world, work.ticket, 0, std::ptr::null(), 0, &mut work).code,
                0
            );
        }
    }
}

#[test]
fn clone_requires_complete_binding_and_copy_is_atomic() {
    unsafe {
        let (world, template) = template();
        let mut root = 0;
        let mut count = 0;
        assert_eq!(
            disunity_clone_create(world, template, 0, &mut root, &mut count).code,
            0
        );
        assert_ne!(root, template);
        let mut rows = checked_rows(world, root, count);
        bind_and_activate(world, root, &rows);
        assert_eq!(disunity_world_destroy(world).code, 0);
        let mut written = 0;
        assert_ne!(
            disunity_clone_copy(world, root, rows.as_mut_ptr(), 2, &mut written).code,
            0
        );
    }
}

/// A prefab template: an object with a script inside a construction that is never activated.
fn prefab_template() -> (u64, u64) {
    unsafe {
        let (mut world, mut construction, mut object, mut script) = (0, 0, 0, 0);
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(
            disunity_construction_begin(world, &mut construction).code,
            0
        );
        assert_eq!(
            disunity_construction_object_create(world, construction, &mut object).code,
            0
        );
        assert_eq!(
            disunity_script_register(world, object, 19, &mut script).code,
            0
        );
        (world, object)
    }
}

#[test]
fn templates_that_never_activate_clone_like_published_objects() {
    unsafe {
        let (world, template) = prefab_template();
        let mut root = 0;
        let mut count = 0;
        assert_eq!(
            disunity_clone_create(world, template, 0, &mut root, &mut count).code,
            0
        );
        let rows = checked_rows(world, root, count);
        assert!(
            rows.iter().any(|row| row.kind == 1),
            "the template's script is cloned"
        );
        bind_and_activate(world, root, &rows);
        assert_eq!(disunity_world_destroy(world).code, 0);
    }
}
