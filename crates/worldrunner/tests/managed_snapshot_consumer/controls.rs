use super::{
    extraction::{Extraction, hash},
    native::{Fixture, Session},
    receipt::ViewReceipt,
    render::Renderer,
    store,
    view::canonical_position,
};
use worldrunner::{glam::Vec3, scene::gpu::device::GpuContext};

pub fn markers(receipt: &ViewReceipt, source: &Extraction, object: u64) {
    assert_eq!(
        receipt.pixel(0, 0),
        [255, 255, 0, 255],
        "caller-authored yellow output stage"
    );
    assert_eq!(
        receipt.pixel(receipt.target.viewport[0] - 1, 0),
        [0, 255, 0, 255]
    );
    let screen = receipt
        .resolved_view
        .world_to_screen(Vec3::from_array(canonical_position(
            source.position(object).unwrap(),
        )))
        .unwrap();
    let [x, y] = screen.truncate().floor().as_uvec2().to_array();
    assert_eq!(
        receipt.pixel(x, y),
        [255, 0, 0, 255],
        "actual managed TRS projects onto rendered geometry"
    );
    assert_eq!(
        receipt.pixels.len(),
        (receipt.target.viewport[0] * receipt.target.viewport[1] * 4) as usize
    );
}

pub fn retained(context: &GpuContext, receipt: &ViewReceipt) {
    assert_eq!(
        store::read(context, &receipt.retained_texture, receipt.target.viewport),
        receipt.pixels
    );
}

pub fn retained_instance(previous: &Extraction, fresh: &Extraction) {
    if previous.stamp.frame > 0 {
        assert_eq!(
            previous.managed.roots, fresh.managed.roots,
            "retained roots across both Update callbacks"
        );
    }
    for key in ["instanceSerial", "awakes"] {
        assert_eq!(fresh.managed.fields()[key], 1);
        assert_eq!(previous.managed.fields()[key], fresh.managed.fields()[key]);
    }
    assert_eq!(fresh.managed.fields()["starts"], 1);
    assert!(
        fresh.managed.fields()["randomWitness"].is_i64(),
        "missing primitive native RNG conservation witness"
    );
    let expected = if fresh.stamp.frame == 1 {
        "expectedFirst"
    } else {
        "expectedSecond"
    };
    assert_eq!(
        fresh.managed.fields()["randomWitness"],
        fresh.managed.fields()[expected]
    );
    assert_eq!(
        fresh.managed.fields()["updates"].as_u64(),
        Some(fresh.stamp.frame)
    );
    assert_eq!(
        fresh.managed.fields()["lateUpdates"].as_u64(),
        Some(fresh.stamp.frame)
    );
    assert!(
        fresh.managed.fields()["callbackSentinel"].as_u64().unwrap()
            > previous.managed.fields()["callbackSentinel"]
                .as_u64()
                .unwrap()
    );
    native_progress(previous, fresh);
}

fn native_progress(previous: &Extraction, fresh: &Extraction) {
    assert_eq!(previous.stamp.world, fresh.stamp.world);
    assert_eq!(
        previous.stamp.world_generation,
        fresh.stamp.world_generation
    );
    assert_eq!(previous.stamp.session, fresh.stamp.session);
    assert_eq!(fresh.stamp.frame, previous.stamp.frame + 1);
    assert_eq!(fresh.stamp.fixed_tick, previous.stamp.fixed_tick + 1);
    assert!(fresh.stamp.content_revision > previous.stamp.content_revision);
    assert!(fresh.stamp.schedule_revision > previous.stamp.schedule_revision);
    assert!(
        fresh.match_observation(&previous.managed).is_err(),
        "stale CLR metadata cannot join a fresh native extraction"
    );
}

pub fn precision(source: &Extraction, session: &Session) {
    let raw = std::str::from_utf8(source.raw()).unwrap();
    assert_eq!(hash(source.raw()), source.stamp.snapshot_sha256);
    let exact = source.captured_value()["objects"][0]["transform"]["position"][0]
        .as_f64()
        .unwrap();
    assert_eq!(exact.to_bits(), 0x3fd7_128d_e000_0000);
    assert_eq!(
        source.position(session.object).unwrap()[0].to_bits(),
        0.360_507_45_f32.to_bits()
    );
    let lossy: serde_json::Value = serde_json::from_str(raw).unwrap();
    let changed = lossy["objects"][0]["transform"]["position"][0]
        .as_f64()
        .unwrap();
    assert_eq!(
        changed.to_bits(),
        exact.to_bits() - 1,
        "actual old codec loses one binary64 ULP"
    );
    assert_ne!(
        hash(&serde_json::to_vec(&lossy).unwrap()),
        source.stamp.snapshot_sha256
    );
    assert_eq!(lossy["objects"][0]["handle"].as_u64(), Some(session.object));
    assert_eq!(
        source.stamp.world_generation,
        u32::try_from(session.world >> 32).unwrap()
    );
    if std::env::var_os("SNAPSHOT_CODEC_LOSS").is_some() {
        assert_eq!(
            changed.to_bits(),
            exact.to_bits(),
            "lossy Value codec rejected"
        );
    }
}

pub fn resize(
    fixture: &Fixture,
    session: &Session,
    source: &Extraction,
    renderer: &mut Renderer<'_>,
    old: &ViewReceipt,
) {
    renderer.target.resize([130, 94]);
    assert!(
        old.bind_current(fixture, session, source, &renderer.target)
            .is_err()
    );
    let resized = renderer
        .capture(fixture, session, source, session.object)
        .unwrap();
    markers(&resized, source, session.object);
    resized
        .bind_current(fixture, session, source, &renderer.target)
        .unwrap();
    resized.retain("update2-130x94");
    renderer.target.resize([65, 47]);
    assert!(
        old.bind_current(fixture, session, source, &renderer.target)
            .is_err(),
        "resize-back cannot reuse old readview generation"
    );
    assert!(source.validate(fixture, session, true).unwrap());
}

pub fn lifecycle(fixture: &mut Fixture, session: &Session, source: &Extraction) {
    let foreign = fixture.foreign_world();
    assert!(
        source
            .validate(
                fixture,
                &Session {
                    world: foreign,
                    object: session.object,
                    script: session.script
                },
                false
            )
            .is_err()
    );
    fixture.destroy(foreign);
    source.release(fixture, session).unwrap();
    assert!(source.validate(fixture, session, false).is_err());
    assert!(source.release(fixture, session).is_err());
    let last = Extraction::capture(fixture, session).unwrap();
    fixture.host.unload().unwrap();
    assert!(
        last.validate(fixture, session, false).is_err(),
        "closed session"
    );
    fixture.destroy(session.world);
    assert!(
        last.validate(fixture, session, false).is_err(),
        "destroyed world"
    );
    let replacement = fixture.foreign_world();
    assert_ne!(replacement, session.world);
    assert!(
        last.validate(
            fixture,
            &Session {
                world: replacement,
                object: session.object,
                script: session.script
            },
            false
        )
        .is_err(),
        "recycled world generation"
    );
    fixture.destroy(replacement);
}
