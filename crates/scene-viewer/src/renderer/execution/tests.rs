#[test]
fn blocked_redraws_do_not_advance_retained_placement_state() {
    use super::super::{Resident, retained_tests::terrain_source};
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let mut resident = Resident::new(
        &context,
        &terrain_source(),
        None,
        crate::FrameProfile::Legacy,
    )
    .unwrap();
    let handle = resident.scene_mut().instance_handles().next().unwrap();
    let flight = FrameAdmission::new(1).unwrap();
    let mut surface = Surface::new();
    let updates = Cell::new(0u32);
    let mut update = || {
        updates.set(updates.get() + 1);
        resident
            .scene_mut()
            .set_instance_visible(handle, updates.get().is_multiple_of(2))?;
        Ok(())
    };
    surface.outcomes.push_back(false);
    assert!(
        acquire_frame(&mut surface, &flight, &mut update)
            .unwrap()
            .is_none()
    );
    let first = acquire_frame(&mut surface, &flight, &mut update)
        .unwrap()
        .unwrap();
    for _ in 0..4 {
        assert!(
            acquire_frame(&mut surface, &flight, &mut update)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(updates.get(), 1);
    drop(first);
    let second = acquire_frame(&mut surface, &flight, &mut update)
        .unwrap()
        .unwrap();
    assert_eq!(updates.get(), 2);
    drop(second);
    assert!(resident.scene_mut().instance(handle).unwrap().1);
}

#[test]
fn close_drops_surface_with_a_real_submission_pending_without_joining() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let flight = FrameAdmission::new(1).unwrap();
    let mut surface = Surface::new();
    let dropped = surface.dropped.clone();
    let (admission, _) = acquire_frame(&mut surface, &flight, || Ok(()))
        .unwrap()
        .unwrap();
    let ticket = flight.submit(admission, context.submission(), [], || {});
    assert!(!ticket.is_complete());
    drop(surface);
    assert!(dropped.get());
    assert!(!ticket.is_complete());
    ticket.wait(context.device()).unwrap();
    assert!(flight.is_idle());
}

use super::*;
use std::{cell::Cell, collections::VecDeque, panic::AssertUnwindSafe, thread::ThreadId};
use worldrunner_render_gpu::surface::{SurfaceAction, SurfaceTransition};

struct Surface {
    owner: ThreadId,
    state: SurfaceTransition,
    outcomes: VecDeque<bool>,
    operations: Vec<(&'static str, [u32; 2])>,
    panic_acquire: bool,
    dropped: Rc<Cell<bool>>,
}
impl Surface {
    fn new() -> Self {
        Self {
            owner: std::thread::current().id(),
            state: SurfaceTransition::new([800, 600]),
            outcomes: VecDeque::new(),
            operations: Vec::new(),
            panic_acquire: false,
            dropped: Rc::new(Cell::new(false)),
        }
    }
}
impl Drop for Surface {
    fn drop(&mut self) {
        assert_eq!(self.owner, std::thread::current().id());
        self.dropped.set(true);
    }
}
impl SurfaceOps for Surface {
    type Frame = [u32; 2];
    fn suspended(&self) -> bool {
        self.state.size().contains(&0)
    }
    fn pending(&self) -> bool {
        self.state.action(0) != SurfaceAction::None
    }
    fn apply(&mut self) -> Result<(), ViewerError> {
        assert_eq!(self.owner, std::thread::current().id());
        let operation = match self.state.action(0) {
            SurfaceAction::Recreate => "recreate",
            SurfaceAction::Configure => "configure",
            other => panic!("unexpected transition {other:?}"),
        };
        self.operations.push((operation, self.state.size()));
        self.state.configured();
        Ok(())
    }
    fn acquire(&mut self) -> Result<Option<Self::Frame>, ViewerError> {
        assert_eq!(self.owner, std::thread::current().id());
        assert!(!self.panic_acquire, "injected acquisition panic");
        Ok(self
            .outcomes
            .pop_front()
            .unwrap_or(true)
            .then(|| self.state.size()))
    }
}

#[test]
fn recovery_and_configuration_run_on_the_calling_event_thread_after_retirement() {
    let mut surface = Surface::new();
    let flight = FrameAdmission::new(2).unwrap();
    let first = flight.try_acquire().unwrap();
    let second = flight.try_acquire().unwrap();
    surface.state.resize([900, 700]);
    surface.state.lost();
    surface.state.resize([640, 480]);
    for _ in 0..3 {
        assert!(
            acquire_frame(&mut surface, &flight, || panic!("blocked update"))
                .unwrap()
                .is_none()
        );
    }
    drop(second);
    assert!(
        acquire_frame(&mut surface, &flight, || panic!("blocked update"))
            .unwrap()
            .is_none()
    );
    assert!(surface.operations.is_empty());
    drop(first);
    let ready = acquire_frame(&mut surface, &flight, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(ready.1, [640, 480]);
    assert_eq!(surface.operations, [("recreate", [640, 480])]);
    drop(ready);
    surface.state.resize([320, 240]);
    acquire_frame(&mut surface, &flight, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(surface.operations[1], ("configure", [320, 240]));
}

#[test]
fn close_with_pending_acquisition_or_submission_has_no_worker_join() {
    for acquired in [false, true] {
        let mut surface = Surface::new();
        let dropped = surface.dropped.clone();
        let flight = FrameAdmission::new(1).unwrap();
        let ready = if acquired {
            acquire_frame(&mut surface, &flight, || Ok(())).unwrap()
        } else {
            None
        };
        surface.state.lost();
        // A surface operation never outlives its caller; close needs no dispatch or acknowledgment.
        drop(ready);
        drop(surface);
        assert!(dropped.get());
        assert!(flight.is_idle());
    }
}

#[test]
fn acquisition_panic_propagates_and_releases_admission_without_a_later_redraw() {
    let mut surface = Surface::new();
    surface.panic_acquire = true;
    let flight = FrameAdmission::new(1).unwrap();
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let _ = acquire_frame(&mut surface, &flight, || panic!("unexpected update"));
    }));
    assert!(result.is_err());
    assert!(flight.is_idle());
}

#[test]
fn updates_require_a_usable_frame_and_never_run_for_blocked_or_suspended_redraws() {
    let mut surface = Surface::new();
    let flight = FrameAdmission::new(2).unwrap();
    let updates = Cell::new(0);
    let update = || {
        updates.set(updates.get() + 1);
        Ok(())
    };
    surface.outcomes.push_back(false);
    assert!(
        acquire_frame(&mut surface, &flight, update)
            .unwrap()
            .is_none()
    );
    assert_eq!(updates.get(), 0);
    let first = acquire_frame(&mut surface, &flight, update)
        .unwrap()
        .unwrap();
    let second = acquire_frame(&mut surface, &flight, update)
        .unwrap()
        .unwrap();
    assert_eq!(updates.get(), 2);
    for _ in 0..8 {
        assert!(
            acquire_frame(&mut surface, &flight, update)
                .unwrap()
                .is_none()
        );
    }
    surface.state.resize([0, 480]);
    surface.state.lost();
    drop(first);
    drop(second);
    assert!(
        acquire_frame(&mut surface, &flight, update)
            .unwrap()
            .is_none()
    );
    assert_eq!(updates.get(), 2);
    surface.state.resize([640, 480]);
    let ready = acquire_frame(&mut surface, &flight, update)
        .unwrap()
        .unwrap();
    assert_eq!(ready.1, [640, 480]);
    assert_eq!(updates.get(), 3);
    assert_eq!(surface.operations, [("recreate", [640, 480])]);
}

#[test]
fn delayed_submission_completion_defers_lifecycle_without_waiting() {
    let context =
        pollster::block_on(GpuContext::request_with_features(wgpu::Features::empty())).unwrap();
    let flight = FrameAdmission::new(1).unwrap();
    let (send, done) = std::sync::mpsc::channel();
    let ticket = flight.submit(
        flight.try_acquire().unwrap(),
        context.submission(),
        [],
        move || {
            send.send(()).unwrap();
        },
    );
    // No later submit or poll can dispatch this callback until the explicit wait below.
    assert!(!ticket.is_complete());
    let mut surface = Surface::new();
    surface.state.resize([900, 700]);
    surface.state.lost();
    surface.state.resize([0, 480]);
    assert!(
        acquire_frame(&mut surface, &flight, || panic!("suspended update"))
            .unwrap()
            .is_none()
    );
    surface.state.resize([640, 480]);
    assert!(
        acquire_frame(&mut surface, &flight, || panic!("pending update"))
            .unwrap()
            .is_none()
    );
    assert!(surface.operations.is_empty());
    ticket.wait(context.device()).unwrap();
    done.recv().unwrap();
    let ready = acquire_frame(&mut surface, &flight, || Ok(()))
        .unwrap()
        .unwrap();
    assert_eq!(ready.1, [640, 480]);
    assert_eq!(surface.operations, [("recreate", [640, 480])]);
}
