use crate::{abi::*, object_api::disunity_component_destroy, store::lock_store};

const INVALID_COMPONENT: u64 = 12;

#[test]
fn a_transform_cannot_be_destroyed_and_the_component_stays() {
    let (mut world, mut object) = (0, 0);
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, 0);
        assert_eq!(disunity_gameobject_create(world, &mut object).code, 0);
    }
    let identity = 4_000_001;
    let transform = {
        let mut store = lock_store();
        let session = store.session_mut(world).unwrap();
        let handle = session.object(object).unwrap();
        let transform = session.native.object(handle).unwrap().transform;
        session.dispatch.components.insert(identity, transform);
        transform
    };
    let refused = disunity_component_destroy(world, identity);
    assert_eq!(
        (refused.code, refused.detail),
        (crate::result_code::ERROR, INVALID_COMPONENT)
    );
    let mut store = lock_store();
    let session = store.session_mut(world).unwrap();
    assert!(session.native.component(transform).is_ok());
    assert_eq!(session.native.storage_counts().1, 1);
    drop(store);
    assert_eq!(unsafe { disunity_world_destroy(world) }.code, 0);
}
