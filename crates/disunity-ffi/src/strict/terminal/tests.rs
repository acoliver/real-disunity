fn claimed_fixed_frame(w: &Fixture, host: &Value) {
    let init = w.call(json!({"op":"phase.open","host":host,"phase":"Initialize",
        "frame":0,"fixed_tick":0}))["data"]["context"]
        .clone();
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"Initialize","context":init}))["ok"],
        true
    );
    assert_eq!(
        w.call(json!({"op":"phase.finish","host":host,"context":init}))["ok"],
        true
    );
    assert_eq!(
        w.call(json!({"op":"frame.open","host":host,"dt":{"num":3,"den":100}}))["ok"],
        true
    );
    let fixed = w.call(json!({"op":"phase.open","host":host,"phase":"FixedUpdate",
        "frame":1,"fixed_tick":1}))["data"]["context"]
        .clone();
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"FixedUpdate","context":fixed}))["ok"],
        true
    );
}

fn projected_body_effects(world: u64) -> Value {
    let mut probe = crate::with_world(world, Clone::clone).unwrap();
    probe.physics_step(0.02).unwrap();
    crate::protocol::read::snapshot(&probe).unwrap()["objects"].clone()
}

#[test]
fn recovery_preserves_latent_body_intent_without_stepping_canonical_world() {
    let w = Fixture::new();
    let h = w.host();
    claimed_fixed_frame(&w, &h);
    for position in [true, false] {
        let o = w.call(json!({"op":"object.create"}))["data"]["handle"].clone();
        let b =
            w.call(json!({"op":"component.add","object":o,"kind":"Rigidbody2D"}))["data"]["handle"]
                .clone();
        let request = if position {
            json!({"op":"body.move","component":b,"position":[9,8]})
        } else {
            json!({"op":"body.force","component":b,"force":[100,0],"mode":"force"})
        };
        assert_eq!(w.call(request)["ok"], true);
    }
    let original = w.call(json!({"op":"snapshot"}));
    let expected = projected_body_effects(w.0);
    assert_ne!(expected, original["data"]["objects"]);
    let r = w.call(json!({"op":"session.begin_cleanup","host":h}));
    assert_eq!(r["ok"], true);
    assert_eq!(projected_body_effects(w.0), expected);
    assert_eq!(
        w.call(json!({"op":"time.get"}))["data"],
        original["data"]["time"]
    );
    let c = &r["data"]["context"];
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"Cleanup","context":c}))["ok"],
        true
    );
    assert_eq!(
        w.call(json!({"op":"phase.finish","host":h,"context":c}))["ok"],
        true
    );
    assert_eq!(projected_body_effects(w.0), expected);
    assert_eq!(
        w.call(json!({"op":"snapshot"}))["data"]["objects"],
        original["data"]["objects"]
    );
}

use crate::{abi, request_value};
use serde_json::{Value, json};

struct Fixture(u64);
impl Fixture {
    fn new() -> Self {
        let mut world = 0;
        assert_eq!(unsafe { abi::disunity_world_create(&mut world) }.code, 0);
        Self(world)
    }
    fn call(&self, request: Value) -> Value {
        request_value(self.0, request).unwrap()
    }
    fn host(&self) -> Value {
        let r = self.call(json!({"op":"session.open","profile":"native_phase_context_v1"}));
        assert_eq!(r["ok"], true);
        json!({"session":r["data"]["session"],"host_key":r["data"]["host_key"]})
    }
    fn counters(&self, claims: u32, sequence: u64, revision: u64) {
        let mut store = crate::lock_store();
        let a = crate::live_session_mut(&mut store, self.0)
            .unwrap()
            .world
            .strict
            .as_mut()
            .unwrap();
        a.claims = claims;
        a.sequence = sequence;
        a.revision = revision;
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        assert_eq!(unsafe { abi::disunity_world_destroy(self.0) }.code, 0);
    }
}

#[test]
fn recovery_claim_is_independent_of_full_gameplay_budget_and_preserves_full_counter() {
    let w = Fixture::new();
    let h = w.host();
    w.counters(super::super::MAX_CLAIMS, 1_u64 << 54, 0);
    let r = w.call(json!({"op":"session.begin_cleanup","host":h}));
    assert_eq!(r["ok"], true);
    let c = &r["data"]["context"];
    assert_eq!(c["sequence"], (1_u64 << 54) + 1);
    let claim = w.call(json!({"op":"phase.claim","phase":"Cleanup","context":c}));
    assert_eq!(claim["ok"], true);
    assert_eq!(claim["data"]["stamp"]["claims"], 4096);
    assert_eq!(claim["data"]["stamp"]["terminal_recovery"]["claims"], 1);
    assert_eq!(
        w.call(json!({"op":"phase.claim","phase":"Cleanup","context":c}))["ok"],
        false
    );
    assert_eq!(
        w.call(json!({"op":"phase.finish","host":h,"context":c}))["data"]["stage"],
        "Closed"
    );
}

#[test]
fn recovery_counter_overflow_fails_atomically_without_consuming_allowance() {
    for (sequence, revision) in [(u64::MAX, 0), (0, u64::MAX)] {
        let w = Fixture::new();
        let h = w.host();
        w.counters(0, sequence, revision);
        let before = w.call(json!({"op":"session.status"}));
        assert_eq!(
            w.call(json!({"op":"session.begin_cleanup","host":h}))["ok"],
            false
        );
        assert_eq!(w.call(json!({"op":"session.status"})), before);
        w.counters(0, 0, 0);
        assert_eq!(
            w.call(json!({"op":"session.begin_cleanup","host":h}))["ok"],
            true
        );
    }
}

#[test]
fn recovery_content_revision_overflow_preserves_world_and_old_authority() {
    let w = Fixture::new();
    let h = w.host();
    {
        let mut store = crate::lock_store();
        crate::live_session_mut(&mut store, w.0)
            .unwrap()
            .world
            .content_revision = u64::MAX;
    }
    let before = w.call(json!({"op":"snapshot"}));
    let status = w.call(json!({"op":"session.status"}));
    assert_eq!(
        w.call(json!({"op":"session.begin_cleanup","host":h}))["ok"],
        false
    );
    assert_eq!(w.call(json!({"op":"snapshot"})), before);
    assert_eq!(w.call(json!({"op":"session.status"})), status);
}
