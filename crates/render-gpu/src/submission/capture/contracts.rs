use std::{cell::RefCell, collections::BTreeSet};

/// CPU resident keys occupy a separate namespace from registered GPU allocations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WriteKey {
    Resident(u64),
    Readback(u64),
    Resource { id: u64, generation: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WriteError {
    #[error("capture publication has no declared key")]
    Unkeyed,
    #[error("capture segment does not own write {0:?}")]
    Undeclared(WriteKey),
    #[error("suffix overwrites prefix without explicit ownership: {0:?}")]
    Overlap(WriteKey),
    #[error("suffix ownership is not in its write set: {0:?}")]
    Ownership(WriteKey),
    #[error("capture publication scope is closed")]
    Closed,
}
/// Complete write contracts are checked before prefix preparation or enqueue.
#[derive(Clone, Debug)]
pub struct CaptureWrites {
    pub(super) prefix: BTreeSet<WriteKey>,
    pub(super) suffix: BTreeSet<WriteKey>,
}
impl CaptureWrites {
    pub fn prefix_keys(&self) -> impl Iterator<Item = WriteKey> + '_ {
        self.prefix.iter().copied()
    }
    pub fn suffix_keys(&self) -> impl Iterator<Item = WriteKey> + '_ {
        self.suffix.iter().copied()
    }
    pub fn new(
        prefix: impl IntoIterator<Item = WriteKey>,
        suffix: impl IntoIterator<Item = WriteKey>,
        suffix_owned: impl IntoIterator<Item = WriteKey>,
    ) -> Result<Self, WriteError> {
        let prefix: BTreeSet<_> = prefix.into_iter().collect();
        let suffix: BTreeSet<_> = suffix.into_iter().collect();
        let owned: BTreeSet<_> = suffix_owned.into_iter().collect();
        if let Some(key) = owned.difference(&suffix).next() {
            return Err(WriteError::Ownership(*key));
        }
        if let Some(key) = prefix
            .intersection(&suffix)
            .find(|key| !owned.contains(key))
        {
            return Err(WriteError::Overlap(*key));
        }
        Ok(Self { prefix, suffix })
    }
}
#[derive(Debug)]
pub(in crate::submission) struct WriteScope {
    allowed: BTreeSet<WriteKey>,
    error: RefCell<Option<WriteError>>,
    active: std::sync::Arc<std::sync::atomic::AtomicBool>,
}
impl WriteScope {
    pub(super) fn new(
        allowed: BTreeSet<WriteKey>,
        active: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> Self {
        Self {
            allowed,
            error: RefCell::default(),
            active,
        }
    }
    pub(in crate::submission) fn retired(&self) -> bool {
        !self.active.load(std::sync::atomic::Ordering::Acquire)
    }
    pub(in crate::submission) fn reject(&self, error: WriteError) -> WriteError {
        *self.error.borrow_mut() = Some(error);
        error
    }
    pub(in crate::submission) fn check(&self, key: WriteKey) -> Result<(), WriteError> {
        if !self.allowed.contains(&key) {
            return Err(self.reject(WriteError::Undeclared(key)));
        }
        Ok(())
    }
    pub(super) fn result(&self) -> Result<(), WriteError> {
        self.error.borrow().map_or(Ok(()), Err)
    }
}
