use super::*;
use std::sync::mpsc;

struct PausedCommands {
    entered: mpsc::Sender<()>,
    resume: mpsc::Receiver<()>,
}
impl IntoIterator for PausedCommands {
    type Item = wgpu::CommandBuffer;
    type IntoIter = std::iter::Empty<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.entered.send(()).unwrap();
        self.resume.recv().unwrap();
        std::iter::empty()
    }
}

fn context() -> (wgpu::Device, wgpu::Queue) {
    let instance = crate::device::headless_instance();
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap()
}

#[test]
fn concurrent_public_submissions_serialize_ticket_registration_and_retirement() {
    let (device, queue) = context();
    let flight = FrameAdmission::new(2).unwrap();
    let first = flight.try_acquire().unwrap();
    let second = flight.try_acquire().unwrap();
    let first_slot = first.slot();
    let second_slot = second.slot();
    let (entered, in_submit) = mpsc::channel();
    let (resume, paused) = mpsc::channel();
    let (observed, locked) = mpsc::channel();
    let (wakes, completed) = mpsc::channel();
    let a_flight = flight.clone();
    let a_queue = queue.clone();
    let a_wakes = wakes.clone();
    let a = std::thread::spawn(move || {
        let wake_flight = a_flight.clone();
        a_flight.submit(
            first,
            &a_queue,
            PausedCommands {
                entered,
                resume: paused,
            },
            move || {
                // Wake is allowed to inspect retirement state without deadlocking registration.
                wake_flight.retire();
                a_wakes.send(0).unwrap();
            },
        )
    });
    in_submit.recv().unwrap();
    let b_flight = flight.clone();
    let b = std::thread::spawn(move || {
        observed
            .send(b_flight.submission.try_lock().is_err())
            .unwrap();
        b_flight.submit(second, &queue, [], move || {
            wakes.send(1).unwrap();
        })
    });
    assert!(locked.recv().unwrap());
    resume.send(()).unwrap();
    let a = a.join().unwrap();
    let b = b.join().unwrap();
    {
        let state = flight.state.lock().unwrap();
        // A may already have retired during B's submission. Any remaining tickets stay ordered.
        let pending: Vec<_> = state.pending.iter().map(|pending| pending.slot).collect();
        assert!(pending == [first_slot, second_slot] || pending == [second_slot]);
        let last = state.pending.back().unwrap();
        assert!(Arc::ptr_eq(&last.ticket.completed, &b.completed));
    }
    a.wait(&device).unwrap();
    b.wait(&device).unwrap();
    assert_eq!(completed.recv().unwrap(), 0);
    assert_eq!(completed.recv().unwrap(), 1);
    assert!(a.is_complete());
    assert!(b.is_complete());
    assert!(flight.is_idle());
}
