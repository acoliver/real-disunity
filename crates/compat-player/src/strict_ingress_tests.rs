use super::*;
use serde_json::json;
use worldrunner::scene::viewer::{IngressOrigin, LiveInputEvent};

fn sample(sequence: u64, nanos: u64, key: &str, pressed: bool) -> LiveInputSample {
    LiveInputSample {
        sequence,
        timestamp: Duration::from_nanos(nanos),
        origin: IngressOrigin::OsEventLoop,
        event: LiveInputEvent::Key {
            key: key.into(),
            pressed,
            repeat: false,
        },
    }
}

#[test]
fn staged_short_tap_preserves_real_time_ids_order_and_filters_host_controls() {
    let mut staged = Staged::default();
    staged.stage(&sample(1, 0, "f12", true)).unwrap();
    staged.stage(&sample(2, 100, "w", true)).unwrap();
    staged.stage(&sample(4, 100, "w", false)).unwrap();
    let batch = staged.batch().unwrap();
    assert_eq!(batch.as_array().unwrap().len(), 2);
    assert_eq!(
        batch[0],
        json!({"event_id":2,"timestamp":{"num":100,"den":1_000_000_000},"source":"os","kind":"key","key":"W","pressed":true})
    );
    assert_eq!(batch[1]["event_id"], 4);
    assert_eq!(batch[1]["pressed"], false);
    staged.accept();
    assert_eq!(staged.batch().unwrap(), json!([]));
}

#[test]
fn complete_batch_rejects_mixed_domains_channel_and_invalid_order() {
    for origin in [
        IngressOrigin::ScheduledReplay,
        IngressOrigin::ChannelDelivery,
    ] {
        let mut staged = Staged::default();
        staged.stage(&sample(1, 100, "w", true)).unwrap();
        let mut other = sample(2, 101, "w", false);
        other.origin = origin;
        staged.stage(&other).unwrap();
        assert!(staged.batch().is_err());
    }
    let mut staged = Staged::default();
    staged.stage(&sample(2, 100, "w", true)).unwrap();
    staged.stage(&sample(1, 101, "w", false)).unwrap();
    assert!(staged.batch().is_err());
}

#[test]
fn reload_retires_queue_and_resets_epoch_domain_and_event_identity() {
    let mut staged = Staged::default();
    staged.stage(&sample(8, 900, "w", true)).unwrap();
    staged.retire(Duration::from_micros(1));
    assert_eq!(staged.batch().unwrap(), json!([]));
    let mut next = sample(10, 1100, "d", true);
    next.origin = IngressOrigin::ScheduledReplay;
    staged.stage(&next).unwrap();
    let batch = staged.batch().unwrap();
    assert_eq!(batch[0]["source"], "replay");
    assert_eq!(batch[0]["timestamp"]["num"], 100);
    assert_eq!(batch[0]["event_id"], 2);
}

#[test]
fn staging_is_bounded_and_repeat_is_a_transition_observation() {
    let mut staged = Staged::default();
    for id in 1..=4096 {
        staged.stage(&sample(id, id, "w", true)).unwrap();
    }
    assert!(staged.stage(&sample(4097, 4097, "w", false)).is_err());
    let mut staged = Staged::default();
    let mut repeated = sample(1, 0, "w", true);
    if let LiveInputEvent::Key { repeat, .. } = &mut repeated.event {
        *repeat = true;
    }
    staged.stage(&repeated).unwrap();
    assert_eq!(staged.batch().unwrap()[0]["pressed"], true);
}

#[test]
fn accepted_batches_keep_order_and_domain_until_retirement() {
    let mut staged = Staged::default();
    staged.stage(&sample(1, 0, "w", true)).unwrap();
    assert_eq!(staged.batch().unwrap()[0]["timestamp"]["num"], 0);
    staged.accept();
    staged.stage(&sample(1, 1, "w", false)).unwrap();
    assert!(staged.batch().is_err());
    assert!(staged.batch().is_err());
    staged.retire(Duration::ZERO);
    let mut replay = sample(2, 1, "d", true);
    replay.origin = IngressOrigin::ScheduledReplay;
    staged.stage(&replay).unwrap();
    assert_eq!(staged.batch().unwrap()[0]["event_id"], 1);
}

#[test]
fn stale_reload_time_and_invalid_filtered_record_reject_the_complete_batch() {
    let mut staged = Staged::default();
    staged.retire(Duration::from_micros(1));
    staged.stage(&sample(1, 999, "w", true)).unwrap();
    assert!(staged.batch().is_err());
    let mut staged = Staged::default();
    staged.stage(&sample(2, 10, "w", true)).unwrap();
    staged.stage(&sample(1, 11, "f12", true)).unwrap();
    assert!(staged.batch().is_err());
}
