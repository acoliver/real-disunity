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
    /// Pages admitted in the current frame.
    admitted: u64,
    /// Pages the current frame may admit; see [`begin_frame`](Self::begin_frame).
    frame_allowance: u64,
    uploads: BTreeMap<u64, Option<u64>>,
    maximum_uploads: u64,
    /// The last frame that began on this cache; see [`frame_began`](Self::frame_began).
    frame: Option<Publications>,
    /// Submission serials of this cache's submitted frames whose completion has not been seen.
    frames: Vec<u64>,
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
            admitted: 0,
            frame_allowance: maximum_uploads,
            uploads: BTreeMap::new(),
            maximum_uploads,
            frame: None,
            frames: Vec::new(),
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
    /// Whether the current frame may admit another page.
    pub fn frame_has_room(&self) -> bool {
        self.admitted < self.frame_allowance
    }
    pub fn admit(&mut self) {
        self.admitted += 1;
    }
    #[cfg(test)]
    pub fn frames_in_flight(&self) -> usize {
        let completed = self.authority.completed_serial();
        self.frames
            .iter()
            .filter(|&&serial| serial > completed)
            .count()
    }
    /// Whether `transaction` is a frame this cache has not seen yet, which then becomes the
    /// current one. Its submission is recorded by [`frame_submitted`](Self::frame_submitted).
    pub fn frame_began(&mut self, transaction: &Publications) -> bool {
        let new = !self
            .frame
            .as_ref()
            .is_some_and(|old| old.same_transaction(transaction));
        if new {
            self.frame = Some(transaction.clone());
            self.retire_frames();
        }
        new
    }
    fn retire_frames(&mut self) {
        let completed = self.authority.completed_serial();
        self.frames.retain(|&serial| serial > completed);
    }
    /// Records the submission of a frame that began on this cache.
    pub fn frame_submitted(&mut self) {
        let serial = self.authority.submitted_serial();
        self.frames.push(serial);
    }
    /// A page admitted now holds its upload scratch until this frame completes, after this
    /// cache's frames in flight, so the scratch turns over once every `1 + frames_in_flight`
    /// frames. Those are the frames that began on this cache, including ones that admitted no
    /// page, and were submitted, and whose completion has not been seen. Other work on the
    /// authority, such as a scene upload's instance flush, prepares no pages and does not count.
    /// Admitting the whole scratch in one frame leaves the following frames none and puts every
    /// bake of a turnover in one frame; this frame takes its share instead, rounded up so the
    /// scratch stays fully used. When the GPU keeps up, no frame is in flight and the share is
    /// the whole per-frame allowance.
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
            self.admitted = 0;
            self.retire_frames();
            self.frame_allowance = self.maximum_uploads.div_ceil(1 + self.frames.len() as u64);
        }
    }
    pub fn capture_write_key(&self) -> WriteKey {
        WriteKey::Resident(self.cache)
    }
    pub fn lookup_for_drawing(&self, key: PageKey) -> Option<ResidentPage> {
        let slot = self.slots[*self.index.get(&key)?]
            .as_ref()
            .expect("indexed slot");
        let same_transaction = self.transaction.as_ref().is_some_and(|owner| {
            Publications::current().is_some_and(|current| current.same_transaction(owner))
        });
        (slot.submitted || same_transaction).then_some(slot.page)
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
    pub fn key(&self) -> PageKey {
        self.page.key
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
