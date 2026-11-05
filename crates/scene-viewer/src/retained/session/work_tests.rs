use super::*;
use crate::{RetainedEventHost, WorkQueue};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, mpsc},
    time::Duration,
};

struct Driver {
    queue: WorkQueue<usize>,
    results: Rc<RefCell<Vec<usize>>>,
    ticks: Rc<Cell<usize>>,
    inputs: Rc<Cell<usize>>,
    thread: std::thread::ThreadId,
}
impl RetainedDriver for Driver {
    fn update(&mut self, _: &mut RetainedHost, _: &RetainedTick) -> Result<(), String> {
        self.ticks.set(self.ticks.get() + 1);
        Ok(())
    }
    fn event(
        &mut self,
        _: &mut RetainedEventHost<'_>,
        event: &RetainedEvent,
    ) -> Result<(), String> {
        assert_eq!(self.thread, std::thread::current().id());
        if matches!(event, RetainedEvent::WorkReady) {
            while let Some(completion) = self.queue.try_recv() {
                self.results.borrow_mut().push(completion.result.unwrap());
            }
        }
        if matches!(event, RetainedEvent::Input(_)) {
            self.inputs.set(self.inputs.get() + 1);
        }
        Ok(())
    }
}

#[test]
fn blocked_work_leaves_ticks_and_input_live_then_wakes_paused_delivery() {
    for pause in [
        RetainedInput::Occluded(true),
        RetainedInput::Resized([0, 0]),
    ] {
        let mut host =
            RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
        let queue = host.work_queue(1, 1).unwrap();
        let (started, running) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        queue
            .submit(move |_| {
                started.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                42
            })
            .unwrap();
        running.recv_timeout(Duration::from_secs(5)).unwrap();
        let results = Rc::new(RefCell::new(Vec::new()));
        let ticks = Rc::new(Cell::new(0));
        let inputs = Rc::new(Cell::new(0));
        let mut driver = Driver {
            queue,
            results: results.clone(),
            ticks: ticks.clone(),
            inputs: inputs.clone(),
            thread: std::thread::current().id(),
        };
        let mut session = Session::new(host, &mut driver);
        let (wake, ready_event) = mpsc::channel();
        session.set_work_waker(Arc::new(move || {
            wake.send(()).unwrap();
        }));
        for _ in 0..3 {
            session.input(RetainedInput::PointerMoved([10.0, 20.0]));
            session.tick().unwrap();
        }
        assert_eq!(ticks.get(), 3);
        assert_eq!(inputs.get(), 3);
        session.input(pause);
        assert!(session.host.view().unwrap().is_none());
        release.send(()).unwrap();
        ready_event.recv_timeout(Duration::from_secs(5)).unwrap();
        session.host_ready().unwrap();
        assert_eq!(*results.borrow(), vec![42]);
        assert_eq!(ticks.get(), 3);
        session.shutdown().unwrap();
        session.host_ready().unwrap();
        assert_eq!(*results.borrow(), vec![42]);
    }
}

#[test]
fn shutdown_discards_ready_results_without_notifying_work() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let queue = host.work_queue(1, 1).unwrap();
    let (wake, ready_event) = mpsc::channel();
    host.work.set_waker(Arc::new(move || {
        wake.send(()).unwrap();
    }));
    queue.submit(|_| 42).unwrap();
    ready_event.recv_timeout(Duration::from_secs(5)).unwrap();
    let results = Rc::new(RefCell::new(Vec::new()));
    let mut driver = Driver {
        queue,
        results: results.clone(),
        ticks: Rc::new(Cell::new(0)),
        inputs: Rc::new(Cell::new(0)),
        thread: std::thread::current().id(),
    };
    let mut session = Session::new(host, &mut driver);
    session.input(RetainedInput::CloseRequested);
    session.host_ready().unwrap();
    assert!(results.borrow().is_empty());
    session.shutdown().unwrap();
    session.host_ready().unwrap();
    assert!(results.borrow().is_empty());
}
