use super::*;

fn key(pressed: bool, repeat: bool) -> LiveInputEvent {
    LiveInputEvent::Key {
        key: "a".into(),
        pressed,
        repeat,
    }
}

#[test]
fn acquisition_timestamps_survive_drain_delay_with_ties_and_preinit_clamping() {
    let origin = Instant::now();
    let mut input = InputBuffer::new(vec![]);
    input.push(LiveInputEvent::Focused(true), origin);
    input.start(origin + Duration::from_secs(10));
    let acquired = origin + Duration::from_secs(11);
    input.push(key(true, false), acquired);
    input.push(key(true, true), acquired);
    input.push(key(false, false), acquired + Duration::from_nanos(1));
    input.push(
        LiveInputEvent::Focused(false),
        acquired + Duration::from_nanos(2),
    );
    let samples = input.collect(0, Duration::ZERO, None).unwrap();
    assert_eq!(
        samples.iter().map(|s| s.timestamp).collect::<Vec<_>>(),
        [
            Duration::ZERO,
            Duration::from_secs(1),
            Duration::from_secs(1),
            Duration::from_secs(1) + Duration::from_nanos(1),
            Duration::from_secs(1) + Duration::from_nanos(2),
        ]
    );
    assert_eq!(
        samples.iter().map(|s| s.event.clone()).collect::<Vec<_>>(),
        [
            LiveInputEvent::Focused(true),
            key(true, false),
            key(true, true),
            key(false, false),
            LiveInputEvent::Focused(false),
        ]
    );
    for (index, sample) in samples.iter().enumerate() {
        assert_eq!(sample.sequence, u64::try_from(index).unwrap() + 1);
        assert_eq!(sample.origin, IngressOrigin::OsEventLoop);
    }
    assert!(input.collect(1, Duration::ZERO, None).unwrap().is_empty());
    input.push(key(true, false), acquired + Duration::from_secs(1));
    let next = input.collect(2, Duration::ZERO, None).unwrap();
    assert_eq!(next[0].sequence, 6);
    assert_eq!(next[0].timestamp, Duration::from_secs(2));
}

#[test]
fn replay_time_preserves_nanoseconds_and_large_frame_numbers() {
    let frames = [0, 3, u64::from(u32::MAX) + 1];
    let events = frames
        .iter()
        .map(|&frame| ScheduledInput {
            frame,
            event: key(true, false),
        })
        .collect();
    let mut input = InputBuffer::new(events);
    input.start(Instant::now());
    let samples = input
        .collect(*frames.last().unwrap(), Duration::from_nanos(17), None)
        .unwrap();
    for (sample, frame) in samples.iter().zip(frames) {
        assert_eq!(sample.timestamp, Duration::from_nanos(frame * 17));
    }
}

#[test]
fn replay_timestamp_overflow_is_diagnostic() {
    let mut input = InputBuffer::new(vec![ScheduledInput {
        frame: 2,
        event: key(true, false),
    }]);
    input.start(Instant::now());
    assert_eq!(
        input.collect(2, Duration::MAX, None).unwrap_err(),
        "live replay timestamp overflow"
    );
}

#[test]
fn channel_budget_is_inclusive_and_never_silently_truncates() {
    for count in [4096, 4097] {
        let (sender, receiver) = std::sync::mpsc::channel();
        for _ in 0..count {
            sender.send(key(true, false)).unwrap();
        }
        let mut input = InputBuffer::new(vec![]);
        input.start(Instant::now());
        let result = input.collect(0, Duration::ZERO, Some(&receiver));
        if count == 4096 {
            let samples = result.unwrap();
            assert_eq!(samples.len(), count);
            assert!(
                samples
                    .iter()
                    .all(|s| s.origin == IngressOrigin::ChannelDelivery)
            );
            assert!(samples.windows(2).all(|s| s[0].timestamp <= s[1].timestamp));
        } else {
            assert_eq!(result.unwrap_err(), BUDGET_ERROR);
        }
    }
}

#[test]
fn queued_and_scheduled_observations_share_the_same_admission_budget() {
    let mut input = InputBuffer::new(vec![ScheduledInput {
        frame: 0,
        event: key(true, false),
    }]);
    let origin = Instant::now();
    input.start(origin);
    for _ in 0..4096 {
        input.push(key(false, false), origin);
    }
    assert_eq!(
        input.collect(0, Duration::ZERO, None).unwrap_err(),
        BUDGET_ERROR
    );
}

#[test]
fn sequence_exhaustion_is_rejected_instead_of_wrapping() {
    let mut input = InputBuffer::new(vec![]);
    let origin = Instant::now();
    input.start(origin);
    input.sequence = u64::MAX;
    input.push(key(true, false), origin);
    assert_eq!(
        input.collect(0, Duration::ZERO, None).unwrap_err(),
        "live ingress sequence exhausted"
    );
}
