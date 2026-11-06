//! Bounded application CPU work, independent of rendering preparation.
use super::RetainedHost;
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
};
mod service;
mod worker;
pub(super) use service::WorkServices;
use service::{Service, Wake};
use worker::{Control, Entry, Job, Shared, State};
#[cfg(test)]
mod tests;

/// Cooperative cancellation. Jobs must poll this token and finish when cancelled.
#[derive(Clone)]
pub struct CancelToken(Arc<AtomicBool>);
impl CancelToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}

/// Queue-scoped, monotonically increasing generation, typed by its result.
pub struct WorkTicket<R> {
    generation: u64,
    owner: Weak<Shared<R>>,
}
impl<R> Clone for WorkTicket<R> {
    fn clone(&self) -> Self {
        Self {
            generation: self.generation,
            owner: self.owner.clone(),
        }
    }
}
impl<R> std::fmt::Debug for WorkTicket<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("WorkTicket").field(&self.generation).finish()
    }
}
impl<R> PartialEq for WorkTicket<R> {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && self.owner.ptr_eq(&other.owner)
    }
}
impl<R> Eq for WorkTicket<R> {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkFailure {
    #[error("CPU job panicked")]
    Panicked,
}
pub struct WorkCompletion<R> {
    pub ticket: WorkTicket<R>,
    pub result: Result<R, WorkFailure>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WorkSubmitError {
    #[error("CPU work queue is full")]
    Full,
    #[error("CPU work queue is closed")]
    Closed,
}
#[derive(Debug, thiserror::Error)]
pub enum WorkQueueError {
    #[error("worker count and queue capacity must be positive and their sum representable")]
    InvalidBounds,
    #[error("CPU host services are closed")]
    Closed,
    #[error("could not start CPU worker: {0}")]
    Spawn(#[from] std::io::Error),
}

/// A fixed worker pool with bounded pending jobs and bounded undelivered results.
/// Drop or host shutdown cancels all jobs and joins workers. Noncooperative jobs
/// therefore delay shutdown. Submission never waits for capacity or job execution.
/// The handle stays on the event thread, so a worker cannot join itself on drop.
pub struct WorkQueue<R: Send + 'static> {
    control: Rc<Control<R>>,
}
impl RetainedHost {
    /// Creates an independent CPU queue. At most `capacity` jobs wait for workers;
    /// at most `workers + capacity` uncancelled tickets, including results, exist.
    pub fn work_queue<R: Send + 'static>(
        &mut self,
        workers: usize,
        capacity: usize,
    ) -> Result<WorkQueue<R>, WorkQueueError> {
        if self.work.closed {
            return Err(WorkQueueError::Closed);
        }
        let limit = workers
            .checked_add(capacity)
            .filter(|_| workers > 0 && capacity > 0)
            .ok_or(WorkQueueError::InvalidBounds)?;
        let (send, receive) = mpsc::sync_channel(capacity);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                send: Some(send),
                live: BTreeMap::new(),
                ready: VecDeque::new(),
                next: 0,
                limit,
            }),
            receive: Mutex::new(receive),
            wake: self.work.wake.clone(),
        });
        let control = Rc::new(Control {
            shared,
            workers: RefCell::new(Vec::new()),
        });
        for _ in 0..workers {
            let shared = control.shared.clone();
            let worker = std::thread::Builder::new()
                .name("retained-cpu".into())
                .spawn(move || worker::run(&shared))?;
            control.workers.borrow_mut().push(worker);
        }
        let service: Rc<dyn Service> = control.clone();
        self.work.register(&service);
        Ok(WorkQueue { control })
    }
}
impl<R: Send + 'static> WorkQueue<R> {
    pub fn submit(
        &self,
        job: impl FnOnce(&CancelToken) -> R + Send + 'static,
    ) -> Result<WorkTicket<R>, WorkSubmitError> {
        let shared = &self.control.shared;
        let mut state = shared.state.lock().unwrap();
        if state.send.is_none() {
            return Err(WorkSubmitError::Closed);
        }
        if state.live.len() == state.limit {
            return Err(WorkSubmitError::Full);
        }
        state.next = state
            .next
            .checked_add(1)
            .expect("CPU ticket generation exhausted");
        let generation = state.next;
        let cancel = CancelToken(Arc::new(AtomicBool::new(false)));
        let job = Job {
            generation,
            cancel: cancel.clone(),
            run: Box::new(job),
        };
        match state.send.as_ref().expect("open queue").try_send(job) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(job)) => {
                drop(state);
                drop(job);
                return Err(WorkSubmitError::Full);
            }
            Err(mpsc::TrySendError::Disconnected(_)) => unreachable!("queue owns its receiver"),
        }
        state.live.insert(
            generation,
            Entry {
                cancel,
                result: None,
            },
        );
        Ok(WorkTicket {
            generation,
            owner: Arc::downgrade(shared),
        })
    }

    /// Invalidates this generation, including a result already awaiting delivery.
    /// Foreign or obsolete tickets return false and cannot affect other work.
    pub fn cancel(&self, ticket: &WorkTicket<R>) -> bool {
        let shared = &self.control.shared;
        if !ticket.owner.ptr_eq(&Arc::downgrade(shared)) {
            return false;
        }
        let mut state = shared.state.lock().unwrap();
        let Some(entry) = state.live.remove(&ticket.generation) else {
            return false;
        };
        entry.cancel.cancel();
        state
            .ready
            .retain(|&generation| generation != ticket.generation);
        drop(state);
        drop(entry);
        true
    }

    /// Cancels the old generation first. If admission returns Full or Closed,
    /// the old job remains cancelled; no replacement ticket has been accepted.
    pub fn replace(
        &self,
        old: &WorkTicket<R>,
        job: impl FnOnce(&CancelToken) -> R + Send + 'static,
    ) -> Result<WorkTicket<R>, WorkSubmitError> {
        self.cancel(old);
        self.submit(job)
    }

    /// Drain from the driver's `WorkReady` event callback, including during pauses.
    pub fn try_recv(&self) -> Option<WorkCompletion<R>> {
        let shared = &self.control.shared;
        let mut state = shared.state.lock().unwrap();
        let generation = state.ready.pop_front()?;
        let entry = state
            .live
            .remove(&generation)
            .expect("ready ticket is live");
        Some(WorkCompletion {
            ticket: WorkTicket {
                generation,
                owner: Arc::downgrade(shared),
            },
            result: entry.result.expect("ready ticket has a result"),
        })
    }
}
