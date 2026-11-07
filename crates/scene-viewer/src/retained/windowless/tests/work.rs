use super::*;
use crate::{RetainedEventHost, WorkQueue, WorkTicket};
use std::sync::{Arc, mpsc};

const WAIT: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Observations {
    events: Vec<&'static str>,
    results: Vec<(WorkTicket<usize>, usize)>,
}

struct WorkDriver {
    queue: Rc<WorkQueue<usize>>,
    observations: Rc<RefCell<Observations>>,
    exit_on_ready: bool,
    thread: std::thread::ThreadId,
}
impl WorkDriver {
    fn new(host: &mut RetainedHost) -> Self {
        Self {
            queue: Rc::new(host.work_queue(1, 1).unwrap()),
            observations: Rc::default(),
            exit_on_ready: false,
            thread: std::thread::current().id(),
        }
    }
}
impl RetainedDriver for WorkDriver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        self.observations.borrow_mut().events.push("update");
        Ok(())
    }
    fn event(
        &mut self,
        host: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        assert_eq!(self.thread, std::thread::current().id());
        let mut observed = self.observations.borrow_mut();
        match event {
            RetainedEvent::WorkReady => {
                observed.events.push("work");
                while let Some(completion) = self.queue.try_recv() {
                    observed
                        .results
                        .push((completion.ticket, completion.result.unwrap()));
                }
                if self.exit_on_ready {
                    host.request_exit();
                }
            }
            RetainedEvent::Presented { .. } => observed.events.push("presented"),
            RetainedEvent::Shutdown => observed.events.push("shutdown"),
            _ => {}
        }
        Ok(())
    }
}

fn ready(run: &mut WindowlessRetained<'_>) -> mpsc::Receiver<()> {
    let (send, receive) = mpsc::channel();
    run.session.set_work_waker(Arc::new(move || {
        send.send(()).unwrap();
    }));
    receive
}

#[test]
fn work_ready_between_frames_precedes_next_update_and_is_delivered_once() {
    let mut host = host(crate::renderer::retained_tests::source());
    let mut driver = WorkDriver::new(&mut host);
    let queue = driver.queue.clone();
    let observed = driver.observations.clone();
    let mut run = host.windowless(&mut driver).unwrap();
    let ready = ready(&mut run);
    assert!(run.frame().unwrap());
    run.renderer.drain().unwrap();
    observed.borrow_mut().events.clear();
    let ticket = queue.submit(|_| 42).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    assert!(run.frame().unwrap());
    assert_eq!(observed.borrow().events, ["work", "update", "presented"]);
    assert_eq!(observed.borrow().results, [(ticket, 42)]);
    run.poll().unwrap();
    assert_eq!(observed.borrow().events, ["work", "update", "presented"]);
    assert!(queue.try_recv().is_none());
    run.shutdown().unwrap();
}

#[test]
fn poll_delivers_work_without_requesting_a_frame_even_when_zero_sized() {
    for size in [[64, 64], [0, 0]] {
        let mut host = host(crate::renderer::retained_tests::source());
        let mut driver = WorkDriver::new(&mut host);
        let queue = driver.queue.clone();
        let observed = driver.observations.clone();
        let mut run = host.windowless(&mut driver).unwrap();
        run.resize(size).unwrap();
        let ready = ready(&mut run);
        let ticket = queue.submit(|_| 42).unwrap();
        ready.recv_timeout(WAIT).unwrap();
        run.poll().unwrap();
        run.poll().unwrap();
        assert_eq!(observed.borrow().events, ["work"]);
        assert_eq!(observed.borrow().results, [(ticket, 42)]);
        assert_eq!(run.session.frame, 0);
        assert!(queue.try_recv().is_none());
        run.shutdown().unwrap();
    }
}

#[test]
fn cancelled_and_replaced_ready_tickets_never_notify_or_deliver_stale_results() {
    let mut host = host(crate::renderer::retained_tests::source());
    let mut driver = WorkDriver::new(&mut host);
    let queue = driver.queue.clone();
    let observed = driver.observations.clone();
    let mut run = host.windowless(&mut driver).unwrap();
    let ready = ready(&mut run);
    let cancelled = queue.submit(|_| 1).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    assert!(queue.cancel(&cancelled));
    run.poll().unwrap();
    assert!(observed.borrow().events.is_empty());
    let old = queue.submit(|_| 2).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let (release, blocked) = mpsc::channel();
    let replacement = queue
        .replace(&old, move |_| {
            blocked.recv_timeout(WAIT).unwrap();
            3
        })
        .unwrap();
    run.frame().unwrap();
    run.poll().unwrap();
    assert_eq!(observed.borrow().events, ["update", "presented"]);
    assert!(observed.borrow().results.is_empty());
    observed.borrow_mut().events.clear();
    release.send(()).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    run.poll().unwrap();
    assert_eq!(observed.borrow().events, ["work"]);
    assert_eq!(observed.borrow().results, [(replacement, 3)]);
    assert!(queue.try_recv().is_none());
    run.shutdown().unwrap();
}

#[test]
fn exit_and_shutdown_discard_ready_work_without_delivery() {
    for exit in [false, true] {
        let mut host = host(crate::renderer::retained_tests::source());
        let mut driver = WorkDriver::new(&mut host);
        let queue = driver.queue.clone();
        let observed = driver.observations.clone();
        let mut run = host.windowless(&mut driver).unwrap();
        let ready = ready(&mut run);
        queue.submit(|_| 42).unwrap();
        ready.recv_timeout(WAIT).unwrap();
        if exit {
            run.host_mut().request_exit();
            assert!(!run.frame().unwrap());
        } else {
            run.shutdown().unwrap();
        }
        assert!(run.poll().is_err());
        assert!(run.frame().is_err());
        assert_eq!(observed.borrow().events, ["shutdown"]);
        assert!(observed.borrow().results.is_empty());
        assert!(queue.try_recv().is_none());
    }
}

#[test]
fn work_ready_exit_shuts_down_before_windowless_update_or_presentation() {
    for frame in [false, true] {
        let mut host = host(crate::renderer::retained_tests::source());
        let mut driver = WorkDriver::new(&mut host);
        driver.exit_on_ready = true;
        let queue = driver.queue.clone();
        let observed = driver.observations.clone();
        let mut run = host.windowless(&mut driver).unwrap();
        let ready = ready(&mut run);
        let ticket = queue.submit(|_| 42).unwrap();
        ready.recv_timeout(WAIT).unwrap();
        assert!(!if frame { run.frame() } else { run.poll() }.unwrap());
        assert_eq!(observed.borrow().events, ["work", "shutdown"]);
        assert_eq!(observed.borrow().results, [(ticket, 42)]);
        assert!(run.poll().is_err());
        assert!(run.frame().is_err());
    }
}
