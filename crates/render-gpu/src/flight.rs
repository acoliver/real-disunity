//! Bounded submission admission. Polling and waiting are explicit host decisions.
#[cfg(test)]
mod concurrency;
use std::collections::VecDeque;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum AdmissionError {
    #[error("frames in flight must be in 1..=3")]
    Capacity,
    #[error("all frame slots are reserved or in flight")]
    WouldBlock,
}

#[derive(Debug)]
struct Pending {
    slot: usize,
    ticket: CompletionTicket,
}

#[derive(Debug)]
struct State {
    occupied: Vec<bool>,
    pending: VecDeque<Pending>,
}

/// Cloneable submission authority for one queue. Reservations count before encoding.
/// All submissions on that queue must use this authority, including unrelated work.
#[derive(Clone, Debug)]
pub struct FrameAdmission {
    state: Arc<Mutex<State>>,
    submission: Arc<Mutex<()>>,
}

/// Dropping an unsubmitted reservation releases its slot without submitting any work.
#[derive(Debug)]
pub struct Admission {
    owner: FrameAdmission,
    slot: Option<usize>,
}
impl Admission {
    pub fn slot(&self) -> usize {
        self.slot.expect("unconsumed admission")
    }
}
impl Drop for Admission {
    fn drop(&mut self) {
        if let Some(slot) = self.slot {
            self.owner.state.lock().unwrap().occupied[slot] = false;
        }
    }
}

/// Identifies exactly one submission, never an implicit wait for the entire queue.
#[derive(Clone, Debug)]
pub struct CompletionTicket {
    submission: wgpu::SubmissionIndex,
    completed: Arc<AtomicBool>,
}
impl CompletionTicket {
    fn complete(&self, wake: impl FnOnce()) {
        self.completed.store(true, Ordering::Release);
        wake();
    }

    pub fn submission(&self) -> wgpu::SubmissionIndex {
        self.submission.clone()
    }
    pub fn is_complete(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
    pub fn wait(&self, device: &wgpu::Device) -> Result<(), wgpu::PollError> {
        device.poll(wgpu::PollType::Wait {
            submission_index: Some(self.submission()),
            timeout: None,
        })?;
        Ok(())
    }
}

impl FrameAdmission {
    pub fn new(capacity: usize) -> Result<Self, AdmissionError> {
        if !(1..=3).contains(&capacity) {
            return Err(AdmissionError::Capacity);
        }
        Ok(Self {
            state: Arc::new(Mutex::new(State {
                occupied: vec![false; capacity],
                pending: VecDeque::new(),
            })),
            submission: Arc::new(Mutex::new(())),
        })
    }

    /// Nonblocking, including when called repeatedly by an input/event loop.
    pub fn try_acquire(&self) -> Result<Admission, AdmissionError> {
        self.retire();
        let mut state = self.state.lock().unwrap();
        let slot = state
            .occupied
            .iter()
            .position(|occupied| !occupied)
            .ok_or(AdmissionError::WouldBlock)?;
        state.occupied[slot] = true;
        Ok(Admission {
            owner: self.clone(),
            slot: Some(slot),
        })
    }

    /// The sole steady-state submission boundary. Wake must not block the callback.
    pub fn submit(
        &self,
        admission: Admission,
        authority: &crate::submission::SubmissionAuthority,
        commands: impl IntoIterator<Item = wgpu::CommandBuffer>,
        wake: impl FnOnce() + Send + 'static,
    ) -> CompletionTicket {
        self.submit_with_completion(admission, authority, commands, |complete| {
            authority.queue().on_submitted_work_done(move || {
                complete();
                wake();
            });
        })
    }

    /// Submits with a host-controlled completion registration boundary.
    /// Register must return without waiting and invoke completion only after this work finishes.
    pub fn submit_with_completion(
        &self,
        mut admission: Admission,
        authority: &crate::submission::SubmissionAuthority,
        commands: impl IntoIterator<Item = wgpu::CommandBuffer>,
        register: impl FnOnce(Box<dyn FnOnce() + Send>),
    ) -> CompletionTicket {
        assert!(
            Arc::ptr_eq(&self.state, &admission.owner.state),
            "foreign admission"
        );
        // Separate from retirement state: a completion wake may inspect or retire slots.
        let _submission = self.submission.lock().unwrap();
        let submission = authority
            .submit_commands(commands)
            .expect("submission admission");
        let ticket = self.track(&mut admission, submission);
        let completed = ticket.clone();
        register(Box::new(move || completed.complete(|| {})));
        ticket
    }

    /// Associates a consuming prepared frame with this admitted flight slot.
    /// Completion registration always uses the authority owned by the frame.
    /// An independent completion authority cannot be supplied:
    /// ```compile_fail
    /// use worldrunner_render_gpu::{flight::{FrameAdmission, Admission}, submission::{PreparedFrame, SubmissionAuthority}};
    /// fn submit(flights: &FrameAdmission, admission: Admission, frame: PreparedFrame, other: &SubmissionAuthority) {
    ///     flights.submit_prepared(admission, frame, other);
    /// }
    /// ```
    pub fn submit_prepared(
        &self,
        mut admission: Admission,
        prepared: crate::submission::PreparedFrame,
    ) -> Result<CompletionTicket, crate::submission::SubmitError> {
        assert!(
            Arc::ptr_eq(&self.state, &admission.owner.state),
            "foreign admission"
        );
        let _submission = self.submission.lock().unwrap();
        let authority = prepared.authority();
        let submission = prepared.submit()?;
        let ticket = self.track(&mut admission, submission);
        let completed = ticket.clone();
        authority
            .queue()
            .on_submitted_work_done(move || completed.complete(|| {}));
        Ok(ticket)
    }

    fn track(
        &self,
        admission: &mut Admission,
        submission: wgpu::SubmissionIndex,
    ) -> CompletionTicket {
        let ticket = CompletionTicket {
            submission,
            completed: Arc::new(AtomicBool::new(false)),
        };
        self.state.lock().unwrap().pending.push_back(Pending {
            slot: admission.slot.take().expect("unconsumed admission"),
            ticket: ticket.clone(),
        });
        ticket
    }

    pub fn retire(&self) {
        let mut state = self.state.lock().unwrap();
        while state
            .pending
            .front()
            .is_some_and(|pending| pending.ticket.is_complete())
        {
            let pending = state.pending.pop_front().unwrap();
            state.occupied[pending.slot] = false;
        }
    }

    pub fn oldest(&self) -> Option<CompletionTicket> {
        self.retire();
        self.state
            .lock()
            .unwrap()
            .pending
            .front()
            .map(|pending| pending.ticket.clone())
    }

    pub fn is_idle(&self) -> bool {
        self.retire();
        !self
            .state
            .lock()
            .unwrap()
            .occupied
            .iter()
            .any(|occupied| *occupied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservations_are_bounded_and_cancellation_releases_only_its_slot() {
        for capacity in 1..=3 {
            let flight = FrameAdmission::new(capacity).unwrap();
            let mut reservations: Vec<_> = (0..capacity)
                .map(|_| flight.try_acquire().unwrap())
                .collect();
            for _ in 0..8 {
                assert_eq!(
                    flight.try_acquire().unwrap_err(),
                    AdmissionError::WouldBlock
                );
            }
            let released = reservations.pop().unwrap();
            let slot = released.slot();
            drop(released);
            assert_eq!(flight.try_acquire().unwrap().slot(), slot);
            drop(reservations);
            assert!(flight.is_idle());
        }
        assert!(matches!(
            FrameAdmission::new(0),
            Err(AdmissionError::Capacity)
        ));
        assert!(matches!(
            FrameAdmission::new(4),
            Err(AdmissionError::Capacity)
        ));
    }

    #[test]
    fn two_submissions_overlap_and_third_requires_oldest_completion() {
        let instance = crate::device::headless_instance();
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .unwrap();
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let authority = crate::submission::SubmissionAuthority::new(queue.clone());
        let flight = FrameAdmission::new(2).unwrap();
        let mut first = flight.try_acquire().unwrap();
        let mut second = flight.try_acquire().unwrap();
        let first_slot = first.slot();
        let (send_a, done_a) = std::sync::mpsc::channel();
        let (send_b, done_b) = std::sync::mpsc::channel();
        let prepared = authority.prepare(&device, |_, _| Ok::<_, ()>(())).unwrap();
        let a = flight.track(&mut first, prepared.submit().unwrap());
        let completion_a = a.clone();
        authority.queue().on_submitted_work_done(move || {
            send_a.send(completion_a).unwrap();
        });
        let prepared = authority.prepare(&device, |_, _| Ok::<_, ()>(())).unwrap();
        let b = flight.track(&mut second, prepared.submit().unwrap());
        let completion_b = b.clone();
        authority.queue().on_submitted_work_done(move || {
            send_b.send(completion_b).unwrap();
        });
        // Explicitly dispatch real queue callbacks, independent of GPU speed or implicit polling.
        assert!(!a.is_complete());
        assert!(!b.is_complete());
        assert_eq!(
            flight.try_acquire().unwrap_err(),
            AdmissionError::WouldBlock
        );
        a.wait(&device).unwrap();
        done_a.recv().unwrap().complete(|| {});
        assert!(a.is_complete());
        assert!(!b.is_complete());
        let third = flight.try_acquire().unwrap();
        assert_eq!(third.slot(), first_slot);
        b.wait(&device).unwrap();
        done_b.recv().unwrap().complete(|| {});
        drop(third);
        assert!(flight.is_idle());
    }
}
