use crate::{abi::*, lock_store, result_code, with_native_world};
use worldrunner_world_simulation::{Advance, CallbackOutcome, TimeSpan};

#[test]
fn existing_abi_and_native_scheduler_share_storage_without_changing_host_contract() {
    let mut world = 0;
    let mut object = 0;
    unsafe {
        assert_eq!(disunity_world_create(&mut world).code, result_code::OK);
        assert_eq!(
            disunity_gameobject_create(world, &mut object).code,
            result_code::OK
        );
        assert_eq!(
            disunity_transform_set_position(world, object, 3.0, 2.0, 1.0).code,
            result_code::OK
        );
        assert_eq!(advance_frame(world, 0.01, 1).code, result_code::OK);
    }
    {
        let mut store = lock_store();
        let session = store.session_mut(world).unwrap();
        let mut work = session.native.advance(TimeSpan::FIXED).unwrap();
        loop {
            match work {
                Advance::Callback(c) => {
                    work = session
                        .native
                        .resume(c.ticket, CallbackOutcome::Returned)
                        .unwrap()
                }
                Advance::Complete(summary) => {
                    assert_eq!(summary.fixed_steps, 1);
                    break;
                }
                Advance::AssetCallback(_) => panic!("unexpected asset callback"),
                Advance::LifecycleComplete => panic!(),
            }
        }
        assert_eq!(
            session.native.snapshot().objects[0]
                .local_transform
                .translation
                .to_array(),
            [3.0, 2.0, 1.0]
        );
    }
    unsafe {
        assert_eq!(advance_frame(world, f32::NAN, 2).code, result_code::ERROR);
    }
    with_native_world(world, |native| {
        assert_eq!(native.clock().time, TimeSpan::new(3, 100).unwrap());
        assert_eq!(native.clock().frame, 2);
    })
    .unwrap();
    unsafe {
        assert_eq!(disunity_world_destroy(world).code, result_code::OK);
    }
}

unsafe fn advance_frame(world: u64, delta: f32, _frame: u64) -> crate::DisunityResult {
    let mut work = crate::scheduling::DisunityAdvance::default();
    let elapsed = std::time::Duration::try_from_secs_f32(delta)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok());
    unsafe {
        crate::scheduling::disunity_world_advance(
            world,
            elapsed.unwrap_or(0),
            if elapsed.is_some() { 1_000_000_000 } else { 0 },
            &mut work,
        )
    }
}
