use std::cell::RefCell;
use std::mem::ManuallyDrop;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};

use super::CompatibilityHost;
use crate::HostError;

static NEXT: AtomicU64 = AtomicU64::new(1);
static REPORT: Mutex<Option<Value>> = Mutex::new(None);
struct Retained {
    id: u64,
    host: Box<ManuallyDrop<CompatibilityHost>>,
}
struct ThreadOwner(&'static RefCell<Option<Retained>>);
thread_local! {
    // The allocation deliberately survives TLS destruction: native worlds and CLR
    // roots cannot migrate to another thread or lose their library/claim on exit.
    static OWNER: ThreadOwner = ThreadOwner(Box::leak(Box::new(RefCell::new(None))));
}
impl Drop for ThreadOwner {
    fn drop(&mut self) {
        if self.0.borrow().is_some() {
            let mut report = REPORT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(report) = report.as_mut() {
                report["owner_thread_alive"] = json!(false);
            }
        }
    }
}
impl CompatibilityHost {
    /// Attempts each retained response release once. Never resubmits a mutation,
    /// copies a response, or invokes managed callbacks. An absent generation is
    /// resolved ownership, but is returned as an error describing the uncertainty.
    pub fn retry_cleanup(&mut self, world: u64) -> Result<(), HostError> {
        if !self.worlds.contains(&world) {
            return Err(HostError::NoWorld);
        }
        let errors = self.native.release_idle_responses(world);
        errors_result(errors)
    }

    /// Latest Drop/recovery report. A retained owner holds the actual library,
    /// process host claim, worlds, response records and private session state.
    /// Only the original live thread can recover it. At most one owner is retained.
    pub fn retained_cleanup_report() -> Option<Value> {
        REPORT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Retries native release/drain and measures managed roots without retrying
    /// callbacks. Failure retains ownership and updates the report. A dead owner
    /// thread permanently pins the owner and claim until process exit.
    pub fn retry_retained_cleanup(id: u64) -> Result<Value, HostError> {
        OWNER.with(|owner| {
            let mut slot = owner.0.borrow_mut();
            let retained = slot.as_mut().filter(|r| r.id == id).ok_or_else(|| {
                HostError::Compatibility(
                    "cleanup owner unavailable on this thread; consult retained_cleanup_report"
                        .into(),
                )
            })?;
            let errors = retained.host.drain_without_callbacks();
            let complete = retained.host.worlds.is_empty() && !retained.host.loaded;
            let mut report = retained.host.cleanup_report(id, !complete, &errors);
            if let Some(previous) = Self::retained_cleanup_report() {
                report["initial_errors"] = previous["initial_errors"].clone();
            }
            *REPORT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(report.clone());
            if complete {
                // SAFETY: drain proved all native worlds and measured managed roots empty.
                unsafe { retained.host.release_fields() };
                slot.take();
            }
            errors_result(errors)?;
            if complete {
                Ok(report)
            } else {
                Err(HostError::Compatibility(
                    "cleanup_incomplete: owner retained".into(),
                ))
            }
        })
    }

    pub(super) fn drain_without_callbacks(&mut self) -> Vec<HostError> {
        let mut errors = Vec::new();
        for world in self.worlds.iter().copied() {
            errors.extend(self.native.release_idle_responses(world));
        }
        match self.measured_empty() {
            Ok(true) => {
                self.loaded = false;
                self.loaded_world = None;
            }
            Ok(false) => {
                errors.push(HostError::Compatibility("cleanup_incomplete: measured managed roots retained; callbacks will not be retried".into()));
                return errors;
            }
            Err(error) => {
                errors.push(error);
                return errors;
            }
        }
        for world in self.worlds.iter().copied().collect::<Vec<_>>() {
            let closed = if self.strict.contains_key(&world) {
                self.close_phase_session(world)
            } else {
                Ok(())
            };
            if let Err(error) = closed {
                errors.push(error);
                continue;
            }
            if let Err(error) = self.native.destroy_world(world) {
                errors.push(error);
                continue;
            }
            self.strict.remove(&world);
            self.worlds.remove(&world);
        }
        errors
    }

    pub(super) fn retain_failed_drop(&mut self, errors: &[HostError]) {
        let id = NEXT
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |id| id.checked_add(1))
            .expect("cleanup owner identity exhausted");
        let mut report = self.cleanup_report(id, true, errors);
        report["initial_errors"] = report["errors"].clone();
        *REPORT
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(report);
        const { assert!(!std::mem::needs_drop::<super::CompatibilityBridge>()) };
        // SAFETY: every owning field is ManuallyDrop. The bridge has no Drop fields.
        // The original Drop leaves those fields untouched; only this retained
        // copy may release them.
        let host = Box::new(ManuallyDrop::new(unsafe { std::ptr::read(self) }));
        let mut retained = Some(Retained { id, host });
        if OWNER
            .try_with(|owner| {
                let mut slot = owner.0.borrow_mut();
                // The retained HostClaim prevents acquisition of a second host.
                assert!(slot.is_none(), "cleanup owner slot already occupied");
                *slot = retained.take();
            })
            .is_err()
        {
            // TLS teardown cannot release a thread-affine native/CLR owner.
            // Its public identity remains in REPORT, with recovery disabled.
            let _pinned = Box::leak(Box::new(retained));
            if let Some(report) = REPORT
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                report["owner_thread_alive"] = json!(false);
            }
        }
    }

    fn cleanup_report(&self, id: u64, retained: bool, errors: &[HostError]) -> Value {
        json!({"id":id,"retained":retained,"owner_thread":format!("{:?}",std::thread::current().id()),"owner_thread_alive":true,"worlds":&*self.worlds,"responses":self.native.idle_response_count(),"managed_loaded":self.loaded,"errors":errors.iter().map(ToString::to_string).collect::<Vec<_>>()})
    }

    pub(super) unsafe fn release_fields(&mut self) {
        // SAFETY: the caller has proved drain completion and invokes this once.
        unsafe {
            ManuallyDrop::drop(&mut self.native);
            ManuallyDrop::drop(&mut self.negotiation);
            ManuallyDrop::drop(&mut self.worlds);
            ManuallyDrop::drop(&mut self.strict);
            ManuallyDrop::drop(&mut self._library);
            ManuallyDrop::drop(&mut self._claim);
        }
    }
}
fn errors_result(errors: Vec<HostError>) -> Result<(), HostError> {
    if errors.is_empty() {
        Ok(())
    } else {
        Err(HostError::Compatibility(
            errors
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("; cleanup: "),
        ))
    }
}
