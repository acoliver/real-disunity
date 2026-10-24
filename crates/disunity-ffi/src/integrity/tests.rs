use super::*;
use serde_json::json;

fn execute(session: &mut Session, request: Value, accepted: bool) -> (Result<Vec<u8>, i32>, usize) {
    let bytes = serde_json::to_vec(&request).unwrap();
    let mut calls = 0;
    let result = session.execute(
        7,
        protocol::Envelope::parse(request).unwrap(),
        &bytes,
        &mut |_| {
            calls += 1;
            if accepted {
                Ok(())
            } else {
                Err(result_code::BUDGET)
            }
        },
    );
    (result, calls)
}

#[test]
fn rejection_preserves_world_and_request_identity_for_retry() {
    let mut session = Session::default();
    let request = json!({"version":1,"requestId":"retry","op":"object.create"});
    let before = session.world.content_revision;
    assert_eq!(
        execute(&mut session, request.clone(), false),
        (Err(result_code::BUDGET), 1)
    );
    assert!(session.world.objects.is_empty());
    assert_eq!(session.world.content_revision, before);
    let bytes = serde_json::to_vec(&request).unwrap();
    assert_eq!(session.ledger.replay("retry", &bytes).unwrap(), None);
    let (response, calls) = execute(&mut session, request.clone(), true);
    assert_eq!(calls, 1);
    assert_eq!(session.world.objects.iter().flatten().count(), 1);
    let revision = session.world.content_revision;
    assert_eq!(execute(&mut session, request, true), (response, 1));
    assert_eq!(session.world.content_revision, revision);
    assert_eq!(session.world.objects.iter().flatten().count(), 1);
}

#[test]
fn replay_requires_fresh_response_admission_without_reexecution() {
    let mut session = Session::default();
    let request = json!({"version":1,"requestId":"retained","op":"object.create"});
    let (response, _) = execute(&mut session, request.clone(), true);
    let revision = session.world.content_revision;
    assert_eq!(
        execute(&mut session, request.clone(), false),
        (Err(result_code::BUDGET), 1)
    );
    assert_eq!(execute(&mut session, request, true), (response, 1));
    assert_eq!(session.world.content_revision, revision);
    assert_eq!(session.world.objects.iter().flatten().count(), 1);
}

#[test]
fn rejected_random_response_does_not_consume_draw_or_request_identity() {
    let mut session = Session::default();
    let request = json!({"version":1,"requestId":"draw","op":"random.range","min":0,"max":1});
    let initial = session.world.random.state;
    assert_eq!(
        execute(&mut session, request.clone(), false),
        (Err(result_code::BUDGET), 1)
    );
    assert_eq!(session.world.random.state, initial);
    let mut oracle = Session::default();
    let (expected, _) = execute(&mut oracle, request.clone(), true);
    assert_eq!(execute(&mut session, request, true), (expected, 1));
    assert_eq!(session.world.random.state, oracle.world.random.state);
}

#[test]
fn identity_conflict_and_wrong_thread_error_replies_require_admission() {
    let mut session = Session::default();
    let first = json!({"version":1,"requestId":"same","op":"object.create"});
    execute(&mut session, first, true).0.unwrap();
    let conflict = json!({"version":1,"requestId":"same","op":"object.create","name":"changed"});
    assert_eq!(
        execute(&mut session, conflict, false),
        (Err(result_code::BUDGET), 1)
    );
    let outcome = std::thread::spawn(move || {
        execute(
            &mut session,
            json!({"version":1,"op":"object.create"}),
            false,
        )
    })
    .join()
    .unwrap();
    assert_eq!(outcome, (Err(result_code::BUDGET), 1));
}

#[test]
fn readonly_preparation_does_not_create_a_staged_world() {
    let mut world = DisunityWorld::default();
    let request = protocol::Envelope::parse(json!({"version":1,"op":"snapshot"})).unwrap();
    let prepared = prepare_response(&mut world, request.request, 7).unwrap();
    assert!(prepared.world.is_none());
    assert!(
        serde_json::from_slice::<Value>(&prepared.response).unwrap()["ok"]
            .as_bool()
            .unwrap()
    );
}
