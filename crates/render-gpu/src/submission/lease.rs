use super::SubmissionAuthority;
use std::{
    any::Any,
    marker::PhantomData,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LeaseError {
    #[error("external frame owner has retired")]
    Expired,
    #[error("external frame is prepared or in flight")]
    Busy,
    #[error("external frame belongs to another submission authority")]
    ForeignAuthority,
}
struct State {
    active: Mutex<bool>,
    uses: AtomicUsize,
    last: AtomicU64,
    authority: SubmissionAuthority,
    owner: Arc<dyn Any + Send + Sync>,
}
/// Capability for a host-owned frame. Clones retain its actual owner, not just its texture.
#[derive(Clone)]
pub struct FrameLease(Arc<State>);
impl std::fmt::Debug for FrameLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameLease")
            .field("active", &*self.0.active.lock().unwrap())
            .field("last_submission", &self.last_submission())
            .finish_non_exhaustive()
    }
}
/// Move the external frame into this owner before importing any of its textures.
/// Retirement returns it only after prepared work and its final GPU use have retired.
pub struct FrameOwner<T: Send + 'static> {
    lease: FrameLease,
    marker: PhantomData<T>,
}
impl<T: Send + 'static> FrameOwner<T> {
    pub fn new(owner: T, authority: &SubmissionAuthority) -> Self {
        Self {
            lease: FrameLease(Arc::new(State {
                active: Mutex::new(true),
                uses: AtomicUsize::new(0),
                last: AtomicU64::new(0),
                authority: authority.clone(),
                owner: Arc::new(Mutex::new(Some(owner))),
            })),
            marker: PhantomData,
        }
    }
    pub fn lease(&self) -> FrameLease {
        self.lease.clone()
    }
    pub fn retire(&mut self) -> Result<T, LeaseError> {
        let mut active = self.lease.0.active.lock().unwrap();
        if !*active {
            return Err(LeaseError::Expired);
        }
        if self.lease.0.uses.load(Ordering::Acquire) != 0
            || self.lease.0.authority.completed_serial() < self.lease.last_submission()
        {
            return Err(LeaseError::Busy);
        }
        *active = false;
        Ok(self
            .lease
            .0
            .owner
            .downcast_ref::<Mutex<Option<T>>>()
            .expect("owner type")
            .lock()
            .unwrap()
            .take()
            .expect("active owner"))
    }
}
impl<T: Send + 'static> Drop for FrameOwner<T> {
    fn drop(&mut self) {
        *self.lease.0.active.lock().unwrap() = false;
    }
}
impl FrameLease {
    pub fn check(&self) -> Result<(), LeaseError> {
        if *self.0.active.lock().unwrap() {
            Ok(())
        } else {
            Err(LeaseError::Expired)
        }
    }
    pub fn same_owner(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn last_submission(&self) -> u64 {
        self.0.last.load(Ordering::Acquire)
    }
    pub(super) fn acquire(&self, authority: &SubmissionAuthority) -> Result<LeaseUse, LeaseError> {
        if !self.0.authority.same_authority(authority) {
            return Err(LeaseError::ForeignAuthority);
        }
        let active = self.0.active.lock().unwrap();
        if !*active {
            return Err(LeaseError::Expired);
        }
        self.0.uses.fetch_add(1, Ordering::AcqRel);
        Ok(LeaseUse(self.clone()))
    }
}
pub(super) struct LeaseUse(FrameLease);
impl LeaseUse {
    pub(super) fn check(&self) -> Result<(), LeaseError> {
        self.0.check()
    }
    pub(super) fn submitted(&self, serial: u64) {
        self.0.0.last.fetch_max(serial, Ordering::AcqRel);
    }
    pub(super) fn same_owner(&self, lease: &FrameLease) -> bool {
        self.0.same_owner(lease)
    }
}
impl Drop for LeaseUse {
    fn drop(&mut self) {
        self.0.0.uses.fetch_sub(1, Ordering::AcqRel);
    }
}
