use super::*;
use serde_json::json;

fn call(session: &mut Session, request: Value, admit: bool) -> Result<Value, i32> {
    let bytes = serde_json::to_vec(&request).unwrap();
    let mut admission = crate::transport::Unpooled;
    if admit {
        return session
            .execute_admitted(
                7,
                protocol::Envelope::parse(request).unwrap(),
                &bytes,
                &mut admission,
            )
            .map(|bytes| serde_json::from_slice(&bytes).unwrap());
    }
    let mut reject = Reject;
    session
        .execute_admitted(
            7,
            protocol::Envelope::parse(request).unwrap(),
            &bytes,
            &mut reject,
        )
        .map(|bytes| serde_json::from_slice(&bytes).unwrap())
}
struct Reject;
impl Admission for Reject {
    fn preflight(&mut self, _: Owner) -> Result<(), i32> {
        Ok(())
    }
    fn admit(&mut self, _: Owner, _: &[u8]) -> Result<(), i32> {
        Err(result_code::BUDGET)
    }
}
fn initialized() -> (Session, Value, Value) {
    let mut session = Session::default();
    let opened = call(
        &mut session,
        json!({"op":"session.open","profile":"native_host_idle_rename_v1"}),
        true,
    )
    .unwrap();
    let host = json!({"session":opened["data"]["session"],"host_key":opened["data"]["host_key"]});
    let object = call(
        &mut session,
        json!({"op":"object.create","name":"before"}),
        true,
    )
    .unwrap()["data"]["handle"]
        .clone();
    let context = call(
        &mut session,
        json!({"op":"phase.open","host":host,"phase":"Initialize","frame":0,"fixed_tick":0}),
        true,
    )
    .unwrap()["data"]["context"]
        .clone();
    call(
        &mut session,
        json!({"op":"phase.claim","phase":"Initialize","context":context}),
        true,
    )
    .unwrap();
    call(
        &mut session,
        json!({"op":"phase.finish","host":host,"context":context}),
        true,
    )
    .unwrap();
    (session, host, object)
}
#[test]
fn idle_rename_response_admission_failure_rolls_back_name_and_revision() {
    let (mut session, host, object) = initialized();
    let before = call(&mut session, json!({"op":"snapshot"}), true).unwrap();
    let request = json!({"op":"host.mutate_idle","host":host,"request":{"op":"object.set","object":object,"name":"after"}});
    assert_eq!(
        call(&mut session, request.clone(), false),
        Err(result_code::BUDGET)
    );
    assert_eq!(
        call(&mut session, json!({"op":"snapshot"}), true).unwrap(),
        before
    );
    assert_eq!(
        call(&mut session, request, true).unwrap()["data"]["name"],
        "after"
    );
}
#[test]
fn idle_rename_content_revision_overflow_rolls_back_name() {
    let (mut session, host, object) = initialized();
    session.world.content_revision = u64::MAX;
    let request = json!({"op":"host.mutate_idle","host":host,"request":{"op":"object.set","object":object,"name":"after"}});
    assert_eq!(call(&mut session, request, true).unwrap()["ok"], false);
    assert_eq!(
        call(
            &mut session,
            json!({"op":"object.get","object":object}),
            true
        )
        .unwrap()["data"]["name"],
        "before"
    );
}
