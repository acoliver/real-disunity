use super::*;
use crate::{AudioStatus, AudioUnavailable, retained::audio::test_support::clip};

#[test]
fn null_device_finishes_only_on_advance_time_even_while_presentation_is_paused() {
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut run = host(crate::renderer::retained_tests::source())
        .windowless(&mut driver)
        .unwrap();
    let status = AudioStatus::Unavailable(AudioUnavailable::NullDevice);
    assert_eq!(run.host_mut().audio().status(), status);
    assert!(
        run.host_mut()
            .audio()
            .start_device_with(|_| panic!("null session cannot start a device"))
            .is_err()
    );
    run.poll().unwrap();
    assert_eq!(
        *events.borrow(),
        [RetainedEvent::AudioStatusChanged { status }]
    );
    events.borrow_mut().clear();
    let clip = run.host_mut().audio().load_wav(&clip()).unwrap();
    let voice = run.host_mut().audio().play(&clip, 1.0).unwrap();
    run.resize([0, 0]).unwrap();
    events.borrow_mut().clear();
    for _ in 0..3 {
        run.poll().unwrap();
    }
    assert!(events.borrow().is_empty());
    run.advance_time(Duration::from_millis(999)).unwrap();
    run.poll().unwrap();
    assert!(events.borrow().is_empty());
    run.advance_time(Duration::from_millis(1)).unwrap();
    assert!(events.borrow().is_empty());
    run.poll().unwrap();
    assert_eq!(*events.borrow(), [RetainedEvent::AudioFinished { voice }]);
    run.poll().unwrap();
    assert_eq!(events.borrow().len(), 1);
    run.shutdown().unwrap();
    assert!(run.advance_time(Duration::from_secs(1)).is_err());
    assert_eq!(run.host_mut().audio().status(), AudioStatus::Closed);
}
