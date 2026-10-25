use super::{
    capture::Capture,
    extraction::{Extraction, Stamp, hash},
    native::{Fixture, Session},
    render::Renderer,
    store,
    view::canonical_position,
};
use serde_json::json;
use worldrunner::{glam::Vec3, scene::gpu::device::GpuContext};

pub fn markers(receipt: &Capture, source: &Extraction, object: u64) {
    assert_eq!(
        receipt.pixel(0, 0),
        [255, 255, 0, 255],
        "caller-authored yellow output stage"
    );
    assert_eq!(
        receipt.pixel(receipt.target.viewport[0] - 1, 0),
        [0, 255, 0, 255],
        "caller-authored green output stage"
    );
    let native = source.position(object).unwrap();
    let screen = receipt
        .resolved_view
        .world_to_screen(Vec3::from_array(canonical_position(native)))
        .unwrap();
    let [x, y] = screen.truncate().floor().as_uvec2().to_array();
    assert_eq!(
        receipt.pixel(x, y),
        [255, 0, 0, 255],
        "native position projects onto rendered geometry"
    );
    assert_eq!(
        receipt.pixels.len(),
        (receipt.target.viewport[0] * receipt.target.viewport[1] * 4) as usize
    );
}

pub fn identities(fixture: &mut Fixture, session: &Session, source: &Extraction) {
    let other = fixture.open();
    fixture.initialize(&other);
    assert!(
        source.validate(fixture, &other, false).is_err(),
        "foreign actual world/session"
    );
    for key in [
        "world",
        "world_generation",
        "session",
        "frame",
        "fixed_tick",
        "schedule_revision",
        "content_revision",
        "snapshot_id",
        "snapshot_sha256",
    ] {
        let mut stamp = serde_json::to_value(&source.stamp).unwrap();
        stamp[key] = if key == "session" {
            json!(fixture.control(&other, json!({"op":"snapshot.capture"}))["stamp"]["session"])
        } else if key == "snapshot_sha256" {
            json!("0".repeat(64))
        } else {
            json!(u64::MAX)
        };
        assert!(
            fixture
                .try_control(
                    session,
                    json!({"op":"snapshot.validate","stamp":stamp,"require_current":false})
                )
                .is_err(),
            "native must reject wrong {key}"
        );
    }
    let mut large = serde_json::to_value(&source.stamp).unwrap();
    large["frame"] = json!(9_007_199_254_740_993_u64);
    let exact: Stamp = serde_json::from_str(&large.to_string()).unwrap();
    assert_eq!(exact.frame, 9_007_199_254_740_993);
    large["frame"] = json!(1.0);
    assert!(
        serde_json::from_value::<Stamp>(large).is_err(),
        "integer identity cannot pass through float"
    );
    fixture.destroy(&other);
}

pub fn copied_payload(fixture: &Fixture, session: &Session, source: &Extraction) {
    let mut tampered = source.captured_value();
    tampered["snapshot"]["objects"][0]["transform"]["position"] = json!([1, 0, 0]);
    assert!(
        Extraction::decode(&tampered)
            .err()
            .unwrap()
            .contains("snapshot digest mismatch")
    );
    assert_eq!(
        source.validate(fixture, session, true).unwrap()["valid"],
        true,
        "native stamp validation alone cannot detect changes to a host copy"
    );
    let before =
        fixture.request(session.world, &json!({"op":"session.status"}))["content_revision"].clone();
    fixture.request(session.world, &json!({"op":"snapshot"}));
    source.validate(fixture, session, true).unwrap();
    assert_eq!(
        fixture.request(session.world, &json!({"op":"session.status"}))["content_revision"],
        before
    );
}

pub fn stale_after_mutation(
    fixture: &Fixture,
    session: &Session,
    source: &Extraction,
    renderer: &mut Renderer<'_>,
    object: u64,
) -> Extraction {
    let retained = source.captured_value();
    fixture.advance(session, object, [1.0, 0.5, 0.0]);
    assert!(
        source.validate(fixture, session, true).is_err(),
        "stale source is never current"
    );
    let historical = source.validate(fixture, session, false).unwrap();
    assert_eq!(historical["valid"], true);
    assert_eq!(
        historical["current"], false,
        "retained historical identity only"
    );
    assert!(
        renderer.capture(fixture, session, source, object).is_err(),
        "stale native source cannot start a GPU capture"
    );
    assert_eq!(
        source.captured_value(),
        retained,
        "host-owned snapshot is immutable after native TRS mutation"
    );
    let fresh = Extraction::capture(fixture, session).unwrap();
    assert_eq!(fresh.stamp.frame, source.stamp.frame + 1);
    assert_eq!(fresh.stamp.fixed_tick, source.stamp.fixed_tick);
    assert!(fresh.stamp.content_revision > source.stamp.content_revision);
    assert_eq!(
        fresh.position(object).unwrap().map(f32::to_bits),
        [1.0_f32, 0.5, 0.0].map(f32::to_bits)
    );
    fresh
}

pub fn retained_pixels(context: &GpuContext, receipt: &Capture) {
    assert_eq!(
        store::read(context, &receipt.retained_texture, receipt.target.viewport),
        receipt.pixels,
        "retained canonical texture preserves completed historical image"
    );
}

pub fn codec_rejects(fixture: &Fixture, session: &Session, object: u64) {
    fixture.advance(session, object, [0.360_507_45, 0.0, 0.0]);
    let captured = fixture.control(session, json!({"op":"snapshot.capture"}));
    let parsed = captured["snapshot"]["objects"][0]["transform"]["position"][0]
        .as_f64()
        .unwrap();
    let expected = f64::from(0.360_507_45_f32);
    assert_ne!(
        parsed.to_bits(),
        expected.to_bits(),
        "actual Value-only transport exhibits the known binary64 ULP loss"
    );
    let reserialized_hash = hash(&serde_json::to_vec(&captured["snapshot"]).unwrap());
    assert_ne!(reserialized_hash, captured["stamp"]["snapshot_sha256"]);
    let stamp = captured["stamp"].clone();
    assert!(
        Extraction::decode(&captured)
            .err()
            .unwrap()
            .contains("snapshot digest mismatch"),
        "consumer fails closed without epsilon"
    );
    eprintln!(
        "codec-negative native f64 bits={:x} runtime parsed bits={:x}; digest rejected",
        expected.to_bits(),
        parsed.to_bits()
    );
    fixture.control(session, json!({"op":"snapshot.release","stamp":stamp}));
    fixture.advance(session, object, [0.5, 1.0, 0.0]);
}

pub fn released_and_dead(
    fixture: &mut Fixture,
    session: &Session,
    source: &Extraction,
    renderer: &mut Renderer<'_>,
    object: u64,
) {
    source.release(fixture, session);
    assert!(source.validate(fixture, session, false).is_err());
    assert!(
        fixture
            .try_control(
                session,
                json!({"op":"snapshot.release","stamp":source.stamp})
            )
            .is_err()
    );
    assert!(
        renderer.capture(fixture, session, source, object).is_err(),
        "released source cannot render"
    );
    let last = Extraction::capture(fixture, session).unwrap();
    fixture.control(session, json!({"op":"session.close"}));
    assert!(
        last.validate(fixture, session, false).is_err(),
        "closed native session rejects retained identity"
    );
    fixture.destroy(session);
    assert!(
        last.validate(fixture, session, false).is_err(),
        "dead world rejects retained identity"
    );
    assert!(renderer.capture(fixture, session, &last, object).is_err());
}

pub fn resize(
    fixture: &Fixture,
    session: &Session,
    source: &Extraction,
    renderer: &mut Renderer<'_>,
    object: u64,
    second: &Capture,
) {
    renderer.target.resize([130, 94]);
    assert!(
        second
            .bind_current(fixture, session, source, &renderer.target)
            .is_err(),
        "resized target invalidates host capture join"
    );
    assert_eq!(
        source.validate(fixture, session, true).unwrap()["current"],
        true
    );
    let resized = renderer.capture(fixture, session, source, object).unwrap();
    markers(&resized, source, object);
    resized
        .bind_current(fixture, session, source, &renderer.target)
        .unwrap();
    renderer.target.resize([65, 47]);
    assert!(
        second
            .bind_current(fixture, session, source, &renderer.target)
            .is_err(),
        "same dimensions after resize still have a different generation"
    );
}
