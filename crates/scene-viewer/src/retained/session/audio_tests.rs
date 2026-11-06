use super::*;
use crate::{AudioStatus, AudioUnavailable, RetainedEventHost};
use std::{cell::RefCell, rc::Rc, time::Duration};

#[derive(Default)]
struct Driver {
    events: Rc<RefCell<Vec<RetainedEvent>>>,
}
impl RetainedDriver for Driver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        Ok(())
    }
    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        let expected = if matches!(event, RetainedEvent::Shutdown) {
            AudioStatus::Closed
        } else {
            AudioStatus::Unavailable(AudioUnavailable::NullDevice)
        };
        assert_eq!(host.audio().status(), expected);
        self.events.borrow_mut().push(event.clone());
        Ok(())
    }
}
use crate::retained::audio::test_support::clip;
#[test]
fn null_audio_completion_uses_session_time_and_shared_hook() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    host.audio.null_device();
    assert_eq!(
        host.audio().status(),
        AudioStatus::Unavailable(AudioUnavailable::NullDevice)
    );
    let clip = host.audio().load_wav(&clip()).unwrap();
    let voice = host.audio().play(&clip, 1.0).unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut session = Session::new(host, &mut driver);
    session.host_ready().unwrap();
    events.borrow_mut().clear();
    session.host.audio.advance(Duration::from_millis(999));
    session.host_ready().unwrap();
    assert!(events.borrow().is_empty());
    session.host.audio.advance(Duration::from_millis(1));
    session.host_ready().unwrap();
    assert_eq!(*events.borrow(), [RetainedEvent::AudioFinished { voice }]);
    session.host_ready().unwrap();
    assert_eq!(events.borrow().len(), 1);
    session.shutdown().unwrap();
    assert!(session.host.audio().play(&clip, 1.0).is_err());
}
