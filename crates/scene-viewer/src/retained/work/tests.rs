use super::*;
use std::{sync::mpsc, time::Duration};

const WAIT: Duration = Duration::from_secs(5);

fn queue<R: Send + 'static>(workers: usize, capacity: usize) -> (RetainedHost, WorkQueue<R>) {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    let queue = host.work_queue(workers, capacity).unwrap();
    (host, queue)
}

fn wake(host: &RetainedHost) -> mpsc::Receiver<()> {
    let (send, receive) = mpsc::channel();
    host.work.set_waker(Arc::new(move || {
        send.send(()).unwrap();
    }));
    receive
}

fn blocked(queue: &WorkQueue<usize>) -> (WorkTicket<usize>, mpsc::Sender<()>) {
    let (started, running) = mpsc::channel();
    let (release, wait) = mpsc::channel();
    let ticket = queue
        .submit(move |_| {
            started.send(()).unwrap();
            wait.recv_timeout(WAIT).unwrap();
            7
        })
        .unwrap();
    running.recv_timeout(WAIT).unwrap();
    (ticket, release)
}

#[test]
fn bounded_queue_returns_full_while_worker_is_blocked() {
    let (host, queue) = queue(1, 1);
    let ready = wake(&host);
    let (first, release) = blocked(&queue);
    let second = queue.submit(|_| 8).unwrap();
    assert!(matches!(queue.submit(|_| 9), Err(WorkSubmitError::Full)));
    assert!(queue.try_recv().is_none());
    release.send(()).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let completion = queue.try_recv().unwrap();
    assert_eq!(completion.ticket, first);
    assert_eq!(completion.result, Ok(7));
    ready.recv_timeout(WAIT).unwrap();
    assert_eq!(queue.try_recv().unwrap().ticket, second);
}

#[test]
fn cancellation_releases_cooperative_work_and_suppresses_completion() {
    let (host, queue) = queue(1, 1);
    let ready = wake(&host);
    let (started, running) = mpsc::channel();
    let (stopped, cancelled) = mpsc::channel();
    let ticket = queue
        .submit(move |cancel| {
            started.send(()).unwrap();
            while !cancel.is_cancelled() {
                std::thread::yield_now();
            }
            stopped.send(()).unwrap();
            1
        })
        .unwrap();
    running.recv_timeout(WAIT).unwrap();
    assert!(queue.cancel(&ticket));
    cancelled.recv_timeout(WAIT).unwrap();
    queue.submit(|_| 2).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    assert_eq!(queue.try_recv().unwrap().result, Ok(2));
    assert!(queue.try_recv().is_none());
}

#[test]
fn panic_is_a_ticket_failure_and_worker_serves_the_next_job() {
    let (host, queue) = queue::<usize>(1, 1);
    let ready = wake(&host);
    let first = queue.submit(|_| panic!("synthetic work failure")).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let completion = queue.try_recv().unwrap();
    assert_eq!(completion.ticket, first);
    assert_eq!(completion.result, Err(WorkFailure::Panicked));
    let second = queue.submit(|_| 42).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let completion = queue.try_recv().unwrap();
    assert_eq!(completion.ticket, second);
    assert_eq!(completion.result, Ok(42));
}

#[test]
fn replacement_rejects_old_ticket_and_discards_already_completed_result() {
    let (host, queue) = queue(1, 1);
    let ready = wake(&host);
    let old = queue.submit(|_| 1).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let new = queue.replace(&old, |_| 2).unwrap();
    assert_ne!(old, new);
    assert!(!queue.cancel(&old));
    ready.recv_timeout(WAIT).unwrap();
    let completion = queue.try_recv().unwrap();
    assert_eq!(completion.ticket, new);
    assert_eq!(completion.result, Ok(2));
    assert!(queue.try_recv().is_none());
}

#[test]
fn running_replacement_cannot_publish_stale_result() {
    let (host, queue) = queue(1, 1);
    let ready = wake(&host);
    let (old, release) = blocked(&queue);
    let new = queue.replace(&old, |_| 9).unwrap();
    release.send(()).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    let completion = queue.try_recv().unwrap();
    assert_eq!(completion.ticket, new);
    assert_eq!(completion.result, Ok(9));
    assert!(queue.try_recv().is_none());
}

#[test]
fn tickets_from_another_queue_cannot_cancel_work() {
    let (_host, first) = queue::<usize>(1, 1);
    let (host, second) = queue(1, 1);
    let ready = wake(&host);
    let ticket = first.submit(|_| 1).unwrap();
    let other = second.submit(|_| 2).unwrap();
    assert!(!second.cancel(&ticket));
    ready.recv_timeout(WAIT).unwrap();
    assert_eq!(second.try_recv().unwrap().ticket, other);
}

#[test]
fn completed_results_count_against_the_bound() {
    let (host, queue) = queue(1, 1);
    let ready = wake(&host);
    queue.submit(|_| 1).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    queue.submit(|_| 2).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    assert!(matches!(queue.submit(|_| 3), Err(WorkSubmitError::Full)));
    queue.try_recv().unwrap();
    queue.submit(|_| 3).unwrap();
    ready.recv_timeout(WAIT).unwrap();
    assert_eq!(queue.try_recv().unwrap().result, Ok(2));
    assert_eq!(queue.try_recv().unwrap().result, Ok(3));
}

#[test]
fn fixed_workers_can_run_jobs_concurrently() {
    let (_host, queue) = queue(2, 1);
    let (_, first) = blocked(&queue);
    let (_, second) = blocked(&queue);
    first.send(()).unwrap();
    second.send(()).unwrap();
}

#[test]
fn host_drop_joins_running_work_discards_queue_and_closes_retained_handle() {
    let (host, queue) = queue::<usize>(1, 1);
    let (started, running) = mpsc::channel();
    let (stopped, cancelled) = mpsc::channel();
    queue
        .submit(move |cancel| {
            started.send(()).unwrap();
            while !cancel.is_cancelled() {
                std::thread::yield_now();
            }
            stopped.send(()).unwrap();
            1
        })
        .unwrap();
    running.recv_timeout(WAIT).unwrap();
    queue
        .submit(|_| panic!("queued work must not run during shutdown"))
        .unwrap();
    drop(host);
    cancelled.try_recv().unwrap();
    assert!(queue.try_recv().is_none());
    assert!(matches!(queue.submit(|_| 2), Err(WorkSubmitError::Closed)));
}

#[test]
fn invalid_bounds_fail_before_starting_workers() {
    let mut host = RetainedHost::new(crate::tests::scene(false), crate::tests::options()).unwrap();
    assert!(matches!(
        host.work_queue::<()>(0, 1),
        Err(WorkQueueError::InvalidBounds)
    ));
    assert!(matches!(
        host.work_queue::<()>(1, 0),
        Err(WorkQueueError::InvalidBounds)
    ));
}
