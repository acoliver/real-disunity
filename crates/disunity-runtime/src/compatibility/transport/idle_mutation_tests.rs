use super::*;
use crate::compatibility::HostIdleMutationError;
use serde_json::json;

unsafe extern "C" fn failed_copy(_: u64, _: *mut u8, _: usize, _: *mut usize) -> DisunityResult {
    DisunityResult { code: 1, detail: 0 }
}
unsafe extern "C" fn failed_free(_: u64) -> DisunityResult {
    DisunityResult { code: 1, detail: 0 }
}
unsafe extern "C" fn malformed_copy(
    _: u64,
    buffer: *mut u8,
    size: usize,
    required: *mut usize,
) -> DisunityResult {
    // SAFETY: the transport supplies writable size and, after probing, buffer outputs.
    unsafe {
        *required = 1;
    }
    if size == 0 {
        return DisunityResult { code: 2, detail: 0 };
    }
    // SAFETY: the transport has allocated the requested byte.
    unsafe {
        *buffer = b'{';
    }
    DisunityResult { code: 0, detail: 0 }
}
fn fixture() -> (std::mem::ManuallyDrop<Library>, NativeTransport, u64, Value) {
    let path = std::env::var("DISUNITY_IDLE_NATIVE_LIBRARY").unwrap();
    // SAFETY: this test explicitly selects the newly built task-owned native image.
    let library = unsafe { Library::new(path) }.unwrap();
    let transport = NativeTransport::bind(&library).unwrap();
    let world = transport.create_world().unwrap();
    let opened = transport
        .request(
            world,
            &json!({"op":"session.open","profile":"native_host_idle_rename_v1"}),
        )
        .unwrap();
    let host = json!({"session":opened["session"],"host_key":opened["host_key"]});
    let object = transport
        .request(world, &json!({"op":"object.create","name":"before"}))
        .unwrap()["handle"]
        .clone();
    let context = transport
        .request(
            world,
            &json!({"op":"phase.open","host":host,"phase":"Initialize","frame":0,"fixed_tick":0}),
        )
        .unwrap()["context"]
        .clone();
    transport
        .request(
            world,
            &json!({"op":"phase.claim","phase":"Initialize","context":context}),
        )
        .unwrap();
    transport
        .request(
            world,
            &json!({"op":"phase.finish","host":host,"context":context}),
        )
        .unwrap();
    (
        std::mem::ManuallyDrop::new(library),
        transport,
        world,
        json!({"version":1,"op":"host.mutate_idle","host":host,"request":{"op":"object.set","object":object,"name":"after"}}),
    )
}
#[test]
fn idle_copy_parse_and_release_uncertainty_never_resubmits_mutation() {
    for mode in 0..3 {
        let (_library, mut transport, world, request) = fixture();
        let (copy, free) = (transport.copy, transport.free);
        let before = transport
            .request(world, &json!({"op":"session.status"}))
            .unwrap()["content_revision"]
            .as_u64()
            .unwrap();
        match mode {
            0 => transport.copy = failed_copy,
            1 => transport.copy = malformed_copy,
            _ => transport.free = failed_free,
        }
        let error = transport
            .request_idle_mutation(world, &request)
            .unwrap_err();
        assert!(matches!(
            error,
            HostIdleMutationError::OutcomeUnknown { .. }
        ));
        transport.copy = copy;
        transport.free = free;
        assert_eq!(
            transport
                .request(
                    world,
                    &json!({"op":"object.get","object":request["request"]["object"]})
                )
                .unwrap()["name"],
            "after"
        );
        assert_eq!(
            transport
                .request(world, &json!({"op":"session.status"}))
                .unwrap()["content_revision"],
            before + 1
        );
        assert!(transport.release_idle_responses(world).is_empty());
        transport.destroy_world(world).unwrap();
    }
}
