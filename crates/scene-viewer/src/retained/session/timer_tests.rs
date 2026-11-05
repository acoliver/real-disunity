use super::*;
use crate::{MAX_HOST_TIMERS, RetainedEventHost, TimerError, TimerHandle};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, mpsc},
    time::Duration,
};

#[derive(Default)]
struct Driver {
    events: Rc<RefCell<Vec<RetainedEvent>>>,
    cancel: Option<TimerHandle>,
    cancel_due: Option<TimerHandle>,
    exit_on_timer: bool,
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
        self.events.borrow_mut().push(event.clone());
        if let RetainedEvent::Timer { handle } = event
            && self.cancel.as_ref() == Some(handle)
        {
            assert!(host.cancel_timer(handle));
            let next = host.timer_at(host.timer_now()).unwrap();
            assert_ne!(*handle, next);
        }
        if matches!(
            event,
            RetainedEvent::WorkReady | RetainedEvent::Timer { .. }
        ) && let Some(handle) = self.cancel_due.take()
        {
            assert!(host.cancel_timer(&handle));
        }
        if self.exit_on_timer && matches!(event, RetainedEvent::Timer { .. }) {
            host.request_exit();
        }
        Ok(())
    }
}
fn host() -> RetainedHost {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    host.timers.freeze();
    host
}
fn advance(session: &mut Session<'_>, millis: u64) {
    session
        .host
        .timers
        .advance(Duration::from_millis(millis))
        .unwrap();
    session.host_ready().unwrap();
}
fn timers(events: &RefCell<Vec<RetainedEvent>>) -> Vec<TimerHandle> {
    events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            RetainedEvent::Timer { handle } => Some(handle.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn one_shot_expires_once_on_the_host_clock() {
    let mut host = host();
    let handle = host
        .timer_at(host.timer_now() + Duration::from_millis(10))
        .unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut session = Session::new(host, &mut driver);
    assert_eq!(
        session.next_wakeup(),
        Some(session.host.timer_now() + Duration::from_millis(10))
    );
    advance(&mut session, 9);
    assert!(timers(&events).is_empty());
    advance(&mut session, 1);
    assert_eq!(timers(&events), std::slice::from_ref(&handle));
    advance(&mut session, 100);
    assert_eq!(timers(&events), [handle]);
    assert_eq!(session.next_wakeup(), None);
}

#[test]
fn repeating_timers_order_by_deadline_then_creation_and_do_not_burst() {
    let mut host = host();
    let later = host.timer_every(Duration::from_millis(20)).unwrap();
    let first = host.timer_every(Duration::from_millis(10)).unwrap();
    let second = host.timer_every(Duration::from_millis(10)).unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut session = Session::new(host, &mut driver);
    advance(&mut session, 1000);
    assert_eq!(
        timers(&events),
        [first.clone(), second.clone(), later.clone()]
    );
    session.host_ready().unwrap();
    assert_eq!(timers(&events).len(), 3);
    advance(&mut session, 10);
    assert_eq!(
        timers(&events),
        [first.clone(), second.clone(), later, first, second]
    );
}

#[test]
fn cancelled_and_foreign_handles_never_deliver_or_cancel_another_timer() {
    let mut host = host();
    let mut foreign = super::timer_tests::host();
    let other = foreign.timer_at(foreign.timer_now()).unwrap();
    let cancelled = host
        .timer_at(host.timer_now() + Duration::from_millis(10))
        .unwrap();
    assert!(!host.cancel_timer(&other));
    assert!(host.cancel_timer(&cancelled));
    assert!(!host.cancel_timer(&cancelled));
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut session = Session::new(host, &mut driver);
    advance(&mut session, 100);
    assert!(timers(&events).is_empty());
}

#[test]
fn cancelling_a_repeating_timer_inside_its_handler_stops_it() {
    let mut host = host();
    let handle = host.timer_every(Duration::from_millis(10)).unwrap();
    let mut driver = Driver {
        cancel: Some(handle.clone()),
        ..Driver::default()
    };
    let events = driver.events.clone();
    let mut session = Session::new(host, &mut driver);
    advance(&mut session, 10);
    assert_eq!(timers(&events), std::slice::from_ref(&handle));
    advance(&mut session, 100);
    assert_eq!(timers(&events).len(), 2);
    assert_eq!(timers(&events)[0], handle);
    assert_eq!(session.next_wakeup(), None);
}

#[test]
fn paused_and_occluded_sessions_deliver_without_ticks() {
    for pause in [
        RetainedInput::Occluded(true),
        RetainedInput::Resized([0, 0]),
    ] {
        let mut host = host();
        let handle = host.timer_every(Duration::from_millis(10)).unwrap();
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut session = Session::new(host, &mut driver);
        session.input(pause);
        assert!(session.host.view().unwrap().is_none());
        advance(&mut session, 10);
        assert_eq!(timers(&events), [handle]);
        assert_eq!(session.frame, 0);
    }
}

#[test]
fn admission_is_bounded_and_rejects_zero_intervals_and_overflow() {
    let mut host = host();
    assert_eq!(
        host.timer_every(Duration::ZERO),
        Err(TimerError::ZeroInterval)
    );
    assert_eq!(
        host.timer_every(Duration::MAX),
        Err(TimerError::DeadlineOverflow)
    );
    let handles: Vec<_> = (0..MAX_HOST_TIMERS)
        .map(|_| host.timer_at(host.timer_now()).unwrap())
        .collect();
    assert_eq!(
        host.timer_at(host.timer_now()),
        Err(TimerError::Full {
            limit: MAX_HOST_TIMERS
        })
    );
    assert!(host.cancel_timer(&handles[0]));
    assert!(host.timer_at(host.timer_now()).is_ok());
}

#[test]
fn exit_and_shutdown_suppress_timers_and_close_admission() {
    for exit in [false, true] {
        let mut host = host();
        host.timer_at(host.timer_now()).unwrap();
        let mut driver = Driver::default();
        let events = driver.events.clone();
        let mut session = Session::new(host, &mut driver);
        if exit {
            session.host.request_exit();
        } else {
            session.shutdown().unwrap();
        }
        session.host_ready().unwrap();
        assert!(timers(&events).is_empty());
        session.shutdown().unwrap();
        assert_eq!(
            session.host.timer_at(session.host.timer_now()),
            Err(TimerError::Closed)
        );
        assert_eq!(session.next_wakeup(), None);
    }
}

#[test]
fn ready_work_precedes_due_timers_in_the_shared_dispatch() {
    let mut host = host();
    let handle = host.timer_at(host.timer_now()).unwrap();
    let queue = host.work_queue(1, 1).unwrap();
    let (send, recv) = mpsc::channel();
    host.work.set_waker(Arc::new(move || {
        send.send(()).unwrap();
    }));
    queue.submit(|_| 42).unwrap();
    recv.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut driver = Driver::default();
    let events = driver.events.clone();
    let mut session = Session::new(host, &mut driver);
    session.host_ready().unwrap();
    assert_eq!(
        *events.borrow(),
        [RetainedEvent::WorkReady, RetainedEvent::Timer { handle }]
    );
    assert_eq!(queue.try_recv().unwrap().result, Ok(42));
}

#[test]
fn callbacks_can_cancel_other_due_timers_before_their_delivery() {
    for work_first in [false, true] {
        let mut host = host();
        let first = host.timer_at(host.timer_now()).unwrap();
        let cancelled = host.timer_at(host.timer_now()).unwrap();
        let queue = host.work_queue(1, 1).unwrap();
        let (send, recv) = mpsc::channel();
        host.work.set_waker(Arc::new(move || {
            send.send(()).unwrap();
        }));
        if work_first {
            queue.submit(|_| 42).unwrap();
            recv.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        let mut driver = Driver {
            cancel_due: Some(cancelled),
            ..Driver::default()
        };
        let events = driver.events.clone();
        let mut session = Session::new(host, &mut driver);
        session.host_ready().unwrap();
        assert_eq!(timers(&events), [first]);
        assert_eq!(events.borrow().len(), if work_first { 2 } else { 1 });
    }
}

#[test]
fn exit_from_a_timer_handler_suppresses_other_due_timers() {
    let mut host = host();
    let first = host.timer_at(host.timer_now()).unwrap();
    host.timer_at(host.timer_now()).unwrap();
    let mut driver = Driver {
        exit_on_timer: true,
        ..Driver::default()
    };
    let events = driver.events.clone();
    let mut session = Session::new(host, &mut driver);
    session.host_ready().unwrap();
    session.host_ready().unwrap();
    assert_eq!(timers(&events), [first]);
    assert_eq!(session.next_wakeup(), None);
}
