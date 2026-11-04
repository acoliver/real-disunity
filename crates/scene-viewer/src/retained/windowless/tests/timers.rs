use super::*;

#[test]
fn timers_advance_deterministically_before_frames_and_during_zero_size_polls() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let start = run.host().timer_now();
    let handle = run
        .host_mut()
        .timer_every(Duration::from_millis(10))
        .unwrap();
    for _ in 0..3 {
        run.poll().unwrap();
    }
    assert_eq!(run.host().timer_now(), start);
    assert!(events.borrow().is_empty());
    run.advance_time(Duration::from_millis(10)).unwrap();
    assert!(events.borrow().is_empty());
    assert!(run.frame().unwrap());
    assert_eq!(
        events.borrow()[0],
        RetainedEvent::Timer {
            handle: handle.clone()
        }
    );
    assert!(matches!(
        events.borrow()[1],
        RetainedEvent::Presented { .. }
    ));
    events.borrow_mut().clear();
    run.resize([0, 0]).unwrap();
    events.borrow_mut().clear();
    run.advance_time(Duration::from_secs(10)).unwrap();
    run.poll().unwrap();
    run.poll().unwrap();
    assert_eq!(*events.borrow(), [RetainedEvent::Timer { handle }]);
    run.shutdown().unwrap();
    assert!(run.advance_time(Duration::from_secs(1)).is_err());
}
