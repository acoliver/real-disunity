use super::{PageCacheError, PageKey};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
use worldrunner_render_gpu::submission::{Publications, SubmissionAuthority, capture::WriteKey};
static NEXT_CACHE: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResidentPage {
    key: PageKey,
    slot: u32,
    token: u64,
    cache: u64,
}
impl ResidentPage {
    pub fn key(self) -> PageKey {
        self.key
    }
    pub fn slot(self) -> u32 {
        self.slot
    }
}
struct Slot {
    page: ResidentPage,
    submitted: bool,
}
pub(in crate::terrain::pages) struct PageState {
    slots: Vec<Option<Slot>>,
    index: BTreeMap<PageKey, usize>,
    free: BTreeSet<usize>,
    token: u64,
    cache: u64,
    transaction: Option<Publications>,
    pub uploaded: u64,
    uploads: BTreeMap<u64, Option<u64>>,
    maximum_uploads: u64,
    authority: SubmissionAuthority,
    #[cfg(test)]
    slot_inspections: std::cell::Cell<usize>,
}
impl PageState {
    pub fn new(slots: u32, maximum_uploads: u64, authority: &SubmissionAuthority) -> Self {
        Self {
            slots: (0..slots).map(|_| None).collect(),
            index: BTreeMap::new(),
            free: (0..slots as usize).collect(),
            token: 0,
            cache: NEXT_CACHE.fetch_add(1, Ordering::Relaxed),
            transaction: None,
            uploaded: 0,
            uploads: BTreeMap::new(),
            maximum_uploads,
            authority: authority.clone(),
            #[cfg(test)]
            slot_inspections: std::cell::Cell::new(0),
        }
    }
    #[cfg(test)]
    fn inspect_slot(&self) {
        self.slot_inspections.set(self.slot_inspections.get() + 1);
    }
    #[cfg(test)]
    pub fn take_slot_inspections(&self) -> usize {
        self.slot_inspections.replace(0)
    }
    pub fn scratch_available(&mut self) -> bool {
        let completed = self.authority.completed_serial();
        self.uploads
            .retain(|_, serial| serial.is_none_or(|serial| serial > completed));
        (self.uploads.len() as u64) < self.maximum_uploads
    }
    pub fn begin_frame(&mut self, transaction: &Publications) {
        if !self
            .transaction
            .as_ref()
            .is_some_and(|old| old.same_transaction(transaction))
        {
            let pending: Vec<_> = self
                .slots
                .iter()
                .inspect(|_| {
                    #[cfg(test)]
                    self.inspect_slot();
                })
                .flatten()
                .filter(|slot| !slot.submitted)
                .map(|slot| slot.page.key)
                .collect();
            for key in pending {
                self.remove(key);
            }
            self.uploads.retain(|_, serial| serial.is_some());
            self.transaction = Some(transaction.clone());
            self.uploaded = 0;
        }
    }
    pub fn keys(&self) -> impl Iterator<Item = PageKey> + '_ {
        self.index.keys().copied()
    }
    pub fn contains(&self, key: PageKey) -> bool {
        self.index.contains_key(&key)
    }
    pub fn reserve(&mut self, key: PageKey) -> Option<ResidentPage> {
        let slot = self.free.pop_first()?;
        assert!(self.index.insert(key, slot).is_none());
        self.token = self.token.checked_add(1).expect("page token overflow");
        let page = ResidentPage {
            key,
            slot: u32::try_from(slot).expect("slot count"),
            token: self.token,
            cache: self.cache,
        };
        self.slots[slot] = Some(Slot {
            page,
            submitted: false,
        });
        self.uploads.insert(page.token, None);
        Some(page)
    }
    pub fn lookup(&self, key: PageKey) -> Option<ResidentPage> {
        let index = *self.index.get(&key)?;
        #[cfg(test)]
        self.inspect_slot();
        let slot = self.slots[index].as_ref().expect("indexed slot");
        slot.submitted.then_some(slot.page)
    }
    pub fn residents(&self) -> impl Iterator<Item = ResidentPage> + '_ {
        self.slots
            .iter()
            .inspect(|_| {
                #[cfg(test)]
                self.inspect_slot();
            })
            .flatten()
            .filter(|slot| slot.submitted)
            .map(|slot| slot.page)
    }
    pub fn is_current(&self, page: ResidentPage) -> bool {
        #[cfg(test)]
        self.inspect_slot();
        self.slots
            .get(page.slot as usize)
            .and_then(Option::as_ref)
            .is_some_and(|slot| slot.submitted && slot.page == page)
    }
    pub fn remove(&mut self, key: PageKey) {
        if let Some(slot) = self.index.remove(&key) {
            self.slots[slot] = None;
            assert!(self.free.insert(slot));
        }
    }
    fn matches(&self, page: ResidentPage) -> bool {
        self.slots
            .get(page.slot as usize)
            .and_then(Option::as_ref)
            .is_some_and(|slot| slot.page == page)
    }
}

/// Reservation owns cancellation cleanup. Only the GPU encoding path can publish it.
pub struct PendingPage {
    state: Rc<RefCell<PageState>>,
    transaction: Publications,
    page: ResidentPage,
    submitted: bool,
}
impl PendingPage {
    pub(super) fn new(
        state: Rc<RefCell<PageState>>,
        transaction: Publications,
        page: ResidentPage,
    ) -> Self {
        Self {
            state,
            transaction,
            page,
            submitted: false,
        }
    }
    pub fn slot(&self) -> u32 {
        self.page.slot
    }
    pub(in crate::terrain::pages) fn validate(
        &self,
        encoder: &wgpu::CommandEncoder,
        state: &Rc<RefCell<PageState>>,
    ) -> Result<(), PageCacheError> {
        let transaction = Publications::for_encoder(encoder)?;
        if !transaction.same_transaction(&self.transaction) {
            return Err(
                worldrunner_render_gpu::submission::TransactionError::ForeignEncoder.into(),
            );
        }
        if !Rc::ptr_eq(&self.state, state) || !self.state.borrow().matches(self.page) {
            return Err(PageCacheError::Address);
        }
        Ok(())
    }
    pub(in crate::terrain::pages) fn stage(
        mut self,
        encoder: &wgpu::CommandEncoder,
    ) -> Result<(), PageCacheError> {
        let mut transaction = Publications::for_encoder(encoder)?;
        if !transaction.same_transaction(&self.transaction) {
            return Err(
                worldrunner_render_gpu::submission::TransactionError::ForeignEncoder.into(),
            );
        }
        transaction.stage_write(WriteKey::Resident(self.page.cache), move || {
            self.submitted = true;
            let mut state = self.state.borrow_mut();
            let serial = state.authority.submitted_serial();
            state.uploads.insert(self.page.token, Some(serial));
            if state.matches(self.page) {
                state.slots[self.page.slot as usize]
                    .as_mut()
                    .unwrap()
                    .submitted = true;
            }
        })?;
        Ok(())
    }
}
impl Drop for PendingPage {
    fn drop(&mut self) {
        let mut state = self.state.borrow_mut();
        if !self.submitted {
            state.uploads.remove(&self.page.token);
        }
        if state.matches(self.page)
            && !state.slots[self.page.slot as usize]
                .as_ref()
                .unwrap()
                .submitted
        {
            state.remove(self.page.key);
        }
    }
}
