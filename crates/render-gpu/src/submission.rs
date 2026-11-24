//! Queue-wide preparation exclusion and the only command submission boundary.
pub mod capture;
mod lease;
mod pass;
mod staging;
pub use lease::FrameLease;
pub use lease::FrameOwner;
pub use lease::LeaseError;
pub use pass::RecordingRenderPass;
pub use staging::StagingStatistics;
use std::cell::Cell;
use std::cell::RefCell;
use std::hash::Hash;
use std::hash::Hasher;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

#[derive(Debug, thiserror::Error)]
pub enum PrepareError<E> {
    #[error("an unsubmitted frame is already prepared")]
    Busy,
    #[error("device lost: {0}")]
    DeviceLost(String),
    #[error("frame preparation failed: {0}")]
    Preparation(E),
}

#[derive(Debug, thiserror::Error)]
pub enum SubmitError {
    #[error(transparent)]
    Lease(#[from] LeaseError),
    #[error(transparent)]
    Transaction(#[from] TransactionError),
    #[error("an unsubmitted frame is already prepared")]
    Busy,
    #[error("device lost: {0}")]
    DeviceLost(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TransactionError {
    #[error("resident encoding requires a prepared transaction")]
    Missing,
    #[error("encoder does not belong to the current transaction")]
    ForeignEncoder,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransactionStatus {
    #[default]
    Preparing,
    Submitted,
    Cancelled,
}

thread_local! {
    static PREPARATION: RefCell<Option<Publications>> = const { RefCell::new(None) };
}

/// Infallible CPU publications. Dropping the set discards all edits.
type PublicationList = Rc<RefCell<Vec<Box<dyn FnOnce()>>>>;

/// The write set of one recording frame. Only [`SubmissionAuthority`] creates one, when a frame
/// begins recording, so every set and every clone handed to a caller belongs to that frame's
/// authority and transaction.
#[derive(Clone)]
pub struct Publications {
    edits: PublicationList,
    status: Rc<Cell<TransactionStatus>>,
    encoder: Rc<Cell<u64>>,
    scope: Option<Rc<capture::WriteScope>>,
    leases: Rc<RefCell<Vec<lease::LeaseUse>>>,
    authority: SubmissionAuthority,
    /// Identifies the recording frame's staging; nonzero.
    transaction: u64,
}
impl std::fmt::Debug for Publications {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Publications")
            .field("count", &self.edits.borrow().len())
            .finish_non_exhaustive()
    }
}
impl<E> PrepareError<E> {
    pub fn into_error(self) -> E
    where
        E: From<SubmitError>,
    {
        match self {
            Self::Busy => SubmitError::Busy.into(),
            Self::DeviceLost(message) => SubmitError::DeviceLost(message).into(),
            Self::Preparation(error) => error,
        }
    }
}
impl Publications {
    pub fn retain_lease(
        encoder: &wgpu::CommandEncoder,
        lease: &FrameLease,
    ) -> Result<(), SubmitError> {
        let current = Self::for_encoder(encoder)?;
        if !current
            .leases
            .borrow()
            .iter()
            .any(|usage| usage.same_owner(lease))
        {
            let usage = lease.acquire(&current.authority)?;
            current.leases.borrow_mut().push(usage);
        }
        Ok(())
    }
    pub fn stage(&mut self, publish: impl FnOnce() + 'static) {
        if let Some(scope) = &self.scope {
            scope.reject(capture::WriteError::Unkeyed);
            return;
        }
        self.edits.borrow_mut().push(Box::new(publish));
    }
    /// Records a declared CPU publication or resource write in a capture segment.
    pub fn write(&self, key: capture::WriteKey) -> Result<(), capture::WriteError> {
        if let Some(scope) = &self.scope {
            if self.status() != TransactionStatus::Preparing {
                return Err(scope.reject(capture::WriteError::Closed));
            }
            scope.check(key)?;
        }
        Ok(())
    }
    pub fn stage_write(
        &mut self,
        key: capture::WriteKey,
        publish: impl FnOnce() + 'static,
    ) -> Result<(), capture::WriteError> {
        self.write(key)?;
        self.edits.borrow_mut().push(Box::new(publish));
        Ok(())
    }
    /// The dynamically scoped write set lets encoder-only adapters stage publication.
    pub fn current() -> Option<Self> {
        PREPARATION.with(|current| current.borrow().clone())
    }
    pub fn for_encoder(encoder: &wgpu::CommandEncoder) -> Result<Self, TransactionError> {
        let current = Self::current().ok_or(TransactionError::Missing)?;
        if current.encoder.get() != encoder_identity(encoder) {
            return Err(TransactionError::ForeignEncoder);
        }
        Ok(current)
    }
    pub fn status(&self) -> TransactionStatus {
        self.status.get()
    }
    /// The authority the transaction's frame was begun from, so work that must follow the frame
    /// (a timestamp resolve, for one) can go through the same exclusion and read its serials.
    pub fn authority(&self) -> &SubmissionAuthority {
        &self.authority
    }
    /// Copies `bytes` into the authority's reusable upload staging and returns the buffer and
    /// offset a copy recorded in this transaction's encoder reads them from. The staging stays
    /// reserved until the transaction is submitted and the GPU has finished with it, or until it
    /// is cancelled. `bytes` must be a nonzero multiple of four bytes long.
    ///
    /// # Panics
    ///
    /// After the transaction was submitted or cancelled, or when `bytes` is longer than the
    /// device's `max_buffer_size`.
    pub fn stage_upload(&self, device: &wgpu::Device, bytes: &[u8]) -> (wgpu::Buffer, u64) {
        assert_eq!(
            self.status(),
            TransactionStatus::Preparing,
            "uploads are staged only while their frame records"
        );
        let authority = &self.authority.0;
        authority
            .staging
            .stage(device, &authority.census, self.transaction, bytes)
    }
    /// Submitted capture slots may be reused only after the lease's final ticket retires.
    pub fn capture_retired(&self) -> bool {
        self.scope.as_ref().is_some_and(|scope| scope.retired())
    }
    pub fn publish(
        encoder: &wgpu::CommandEncoder,
        publish: impl FnOnce() + 'static,
    ) -> Result<(), TransactionError> {
        Self::for_encoder(encoder)?.stage(publish);
        Ok(())
    }
    pub fn same_transaction(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.edits, &other.edits)
    }
    /// A live preparation or two segments of the same owned capture frame.
    pub fn same_frame(&self, other: &Self) -> bool {
        (self.same_transaction(other) && self.status() == TransactionStatus::Preparing)
            || matches!(
                (&self.scope, &other.scope),
                (Some(left), Some(right)) if left.same_capture(right)
            )
    }
}

#[derive(Debug, Default)]
struct State {
    preparing: bool,
    configuring: bool,
    capture_cleanup: Option<capture::Cleanup>,
}
#[derive(Debug)]
struct Inner {
    queue: wgpu::Queue,
    state: Mutex<State>,
    in_flight: AtomicUsize,
    lost: Arc<Mutex<Option<String>>>,
    submitted: AtomicU64,
    completed: AtomicU64,
    /// Serials whose completion callback has not finished all of its bookkeeping. A callback
    /// removes its serial last, so a serial missing here has advanced `completed`, released
    /// its `in_flight` count and dropped its leases.
    unsettled: Mutex<std::collections::BTreeSet<u64>>,
    census: crate::census::MemoryCensus,
    staging: staging::Staging,
    transactions: AtomicU64,
}

/// Owns a queue and shares preparation and submission exclusion through clones.
#[derive(Clone, Debug)]
pub struct SubmissionAuthority(Arc<Inner>);
impl SubmissionAuthority {
    pub fn new(queue: wgpu::Queue) -> Self {
        Self(Arc::new(Inner {
            queue,
            state: Mutex::new(State::default()),
            in_flight: AtomicUsize::new(0),
            lost: Arc::new(Mutex::new(None)),
            submitted: AtomicU64::new(0),
            completed: AtomicU64::new(0),
            unsettled: Mutex::default(),
            census: crate::census::MemoryCensus::new(),
            staging: staging::Staging::default(),
            transactions: AtomicU64::new(0),
        }))
    }
    /// The resident-byte tally for resources created on this queue's device. Clones of the
    /// authority share it.
    pub fn census(&self) -> &crate::census::MemoryCensus {
        &self.0.census
    }
    /// Counters of the reusable upload staging behind [`Publications::stage_upload`].
    pub fn upload_staging(&self) -> StagingStatistics {
        self.0.staging.statistics()
    }
    pub fn same_authority(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    /// Queue access is for initialization uploads and completion/presentation only.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.0.queue
    }
    pub fn report_device_loss(&self, message: String) {
        *self.0.lost.lock().unwrap() = Some(message);
    }
    /// Reports loss without retaining the queue, which itself retains the device.
    pub fn device_loss_reporter(&self) -> impl Fn(String) + Send + 'static {
        let lost = Arc::clone(&self.0.lost);
        move |message| *lost.lock().unwrap() = Some(message)
    }
    pub fn check_device(&self) -> Result<(), SubmitError> {
        if let Some(lost) = &*self.0.lost.lock().unwrap() {
            return Err(SubmitError::DeviceLost(lost.clone()));
        }
        Ok(())
    }
    pub fn is_idle(&self) -> bool {
        self.retire_capture();
        let state = self.0.state.lock().unwrap();
        !state.preparing && !state.configuring && self.0.in_flight.load(Ordering::Acquire) == 0
    }
    /// Reserves queue-wide idle exclusion until the guard is dropped.
    pub fn try_configuration(&self) -> Result<ConfigurationGuard, SubmitError> {
        self.retire_capture();
        self.check_device()?;
        let mut state = self.0.state.lock().unwrap();
        if state.preparing || state.configuring || self.0.in_flight.load(Ordering::Acquire) != 0 {
            return Err(SubmitError::Busy);
        }
        state.configuring = true;
        Ok(ConfigurationGuard(self.clone()))
    }
    /// Replacements exclude unsubmitted work but permit older generations in flight.
    pub fn try_replacement(&self) -> Result<ConfigurationGuard, SubmitError> {
        self.retire_capture();
        let mut state = self.0.state.lock().unwrap();
        if let Some(lost) = &*self.0.lost.lock().unwrap() {
            return Err(SubmitError::DeviceLost(lost.clone()));
        }
        if state.preparing || state.configuring {
            return Err(SubmitError::Busy);
        }
        state.configuring = true;
        Ok(ConfigurationGuard(self.clone()))
    }
    pub fn submitted_serial(&self) -> u64 {
        self.0.submitted.load(Ordering::Acquire)
    }
    pub fn completed_serial(&self) -> u64 {
        self.0.completed.load(Ordering::Acquire)
    }
    /// Whether every submission up to `serial` has finished its completion bookkeeping. The
    /// callback that does it runs on whichever thread's poll collected it, which can be after
    /// another thread's wait for the same submission has returned.
    pub fn settled_through(&self, serial: u64) -> bool {
        self.0
            .unsettled
            .lock()
            .unwrap()
            .first()
            .is_none_or(|&first| first > serial)
    }
    pub fn begin(&self, device: &wgpu::Device) -> Result<RecordingFrame, SubmitError> {
        self.retire_capture();
        {
            let mut state = self.0.state.lock().unwrap();
            if let Some(lost) = &*self.0.lost.lock().unwrap() {
                return Err(SubmitError::DeviceLost(lost.clone()));
            }
            if state.preparing || state.configuring {
                return Err(SubmitError::Busy);
            }
            state.preparing = true;
        }
        Ok(self.record(device, None, true))
    }
    /// Begins recording a frame with a new transaction's write set, the only place one is made.
    fn record(
        &self,
        device: &wgpu::Device,
        scope: Option<Rc<capture::WriteScope>>,
        reserved: bool,
    ) -> RecordingFrame {
        let publications = Publications {
            edits: PublicationList::default(),
            status: Rc::default(),
            encoder: Rc::default(),
            scope,
            leases: Rc::default(),
            authority: self.clone(),
            transaction: self.0.transactions.fetch_add(1, Ordering::AcqRel) + 1,
        };
        let frame = PreparedFrame {
            authority: self.clone(),
            commands: None,
            publications,
            reserved,
            valid_encoder: true,
        };
        let encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("prepared frame"),
        });
        frame.publications.encoder.set(encoder_identity(&encoder));
        let previous =
            PREPARATION.with(|current| current.replace(Some(frame.publications.clone())));
        RecordingFrame {
            frame: Some(frame),
            encoder: Some(encoder),
            previous,
        }
    }
    pub fn prepare<E>(
        &self,
        device: &wgpu::Device,
        prepare: impl FnOnce(&mut wgpu::CommandEncoder, &mut Publications) -> Result<(), E>,
    ) -> Result<PreparedFrame, PrepareError<E>> {
        let mut recording = self.begin(device).map_err(|error| match error {
            SubmitError::Busy => PrepareError::Busy,
            SubmitError::DeviceLost(message) => PrepareError::DeviceLost(message),
            SubmitError::Transaction(_) | SubmitError::Lease(_) => {
                unreachable!("begin does not accept an encoder or lease")
            }
        })?;
        let mut publications = recording.publications();
        prepare(&mut recording, &mut publications).map_err(PrepareError::Preparation)?;
        Ok(recording.finish())
    }
    /// Convenience work shares exclusion with prepared frames, including load and capture work.
    pub fn submit_commands(
        &self,
        commands: impl IntoIterator<Item = wgpu::CommandBuffer>,
    ) -> Result<wgpu::SubmissionIndex, SubmitError> {
        self.retire_capture();
        let mut state = self.0.state.lock().unwrap();
        if state.preparing || state.configuring {
            return Err(SubmitError::Busy);
        }
        self.enqueue(&mut state, commands, Vec::new(), 0)
    }
    fn enqueue(
        &self,
        _state: &mut State,
        commands: impl IntoIterator<Item = wgpu::CommandBuffer>,
        leases: Vec<lease::LeaseUse>,
        transaction: u64,
    ) -> Result<wgpu::SubmissionIndex, SubmitError> {
        for usage in &leases {
            usage.check()?;
        }
        if let Some(lost) = &*self.0.lost.lock().unwrap() {
            return Err(SubmitError::DeviceLost(lost.clone()));
        }
        let queue = &self.0.queue;
        // Staged uploads must be unmapped for the queue, and are mapped again only after it.
        let staged = self.0.staging.unmap(transaction);
        let submission = queue.submit(commands);
        staging::Staging::remap(staged);
        self.0.in_flight.fetch_add(1, Ordering::Release);
        let serial = self.0.submitted.fetch_add(1, Ordering::AcqRel) + 1;
        for usage in &leases {
            usage.submitted(serial);
        }
        self.0.unsettled.lock().unwrap().insert(serial);
        // Native enqueue may dispatch earlier callbacks synchronously. Never take the submission
        // lock here, and hold no lock across the registration.
        let inner = Arc::downgrade(&self.0);
        queue.on_submitted_work_done(move || {
            let retained = leases;
            if let Some(inner) = inner.upgrade() {
                inner.completed.fetch_max(serial, Ordering::AcqRel);
                inner.in_flight.fetch_sub(1, Ordering::AcqRel);
                drop(retained);
                inner.unsettled.lock().unwrap().remove(&serial);
            }
        });
        Ok(submission)
    }
}

/// Owns exactly one unsubmitted encoder and its resident write set.
/// Submission and cancellation consume the handle. Drop never submits work.
#[must_use = "submit or cancel the prepared frame"]
pub struct PreparedFrame {
    authority: SubmissionAuthority,
    commands: Option<wgpu::CommandBuffer>,
    publications: Publications,
    reserved: bool,
    valid_encoder: bool,
}
impl std::fmt::Debug for PreparedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreparedFrame").finish_non_exhaustive()
    }
}
impl PreparedFrame {
    pub fn cancel(self) {}
    pub(crate) fn authority(&self) -> SubmissionAuthority {
        self.authority.clone()
    }
    pub fn submit(mut self) -> Result<wgpu::SubmissionIndex, SubmitError> {
        let submission = self.submit_retained()?;
        self.authority.0.state.lock().unwrap().preparing = false;
        self.reserved = false;
        Ok(submission)
    }
    fn submit_retained(&mut self) -> Result<wgpu::SubmissionIndex, SubmitError> {
        let submission = self.enqueue_commands()?;
        self.commit();
        Ok(submission)
    }
    fn enqueue_commands(&mut self) -> Result<wgpu::SubmissionIndex, SubmitError> {
        if !self.valid_encoder {
            return Err(TransactionError::ForeignEncoder.into());
        }
        let mut state = self.authority.0.state.lock().unwrap();
        let submission = self.authority.enqueue(
            &mut state,
            self.commands.take(),
            self.publications.leases.take(),
            self.publications.transaction,
        )?;
        drop(state);
        Ok(submission)
    }
    fn commit(&mut self) {
        self.publications.status.set(TransactionStatus::Submitted);
        for publish in self.publications.edits.take() {
            publish();
        }
    }
}
impl Drop for PreparedFrame {
    fn drop(&mut self) {
        if self.publications.status() == TransactionStatus::Preparing {
            self.publications.status.set(TransactionStatus::Cancelled);
        }
        self.publications.edits.borrow_mut().clear();
        self.publications.leases.borrow_mut().clear();
        // Only a frame that never reached the queue still holds mapped, recording staging.
        if self.publications.status() == TransactionStatus::Cancelled {
            self.authority
                .0
                .staging
                .cancel(self.publications.transaction);
        }
        if self.reserved {
            self.authority
                .0
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .preparing = false;
        }
    }
}

fn encoder_identity(encoder: &wgpu::CommandEncoder) -> u64 {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    encoder.hash(&mut hash);
    hash.finish()
}

/// Holds submission exclusion across native surface configuration.
pub struct ConfigurationGuard(SubmissionAuthority);
impl Drop for ConfigurationGuard {
    fn drop(&mut self) {
        self.0
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .configuring = false;
    }
}

/// Owns the encoder and its bound publication scope while recording.
/// Finish returns a consuming frame; dropping either handle cancels publication.
pub struct RecordingFrame {
    frame: Option<PreparedFrame>,
    encoder: Option<wgpu::CommandEncoder>,
    previous: Option<Publications>,
}
impl RecordingFrame {
    /// The frame's write set, for the callback that records it.
    fn publications(&self) -> Publications {
        self.frame.as_ref().unwrap().publications.clone()
    }
    pub fn finish(mut self) -> PreparedFrame {
        let mut frame = self.frame.take().unwrap();
        let encoder = self.encoder.take().unwrap();
        frame.valid_encoder = frame.publications.encoder.get() == encoder_identity(&encoder);
        frame.commands = Some(encoder.finish());
        frame
    }
}
impl std::ops::Deref for RecordingFrame {
    type Target = wgpu::CommandEncoder;
    fn deref(&self) -> &Self::Target {
        self.encoder.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for RecordingFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.encoder.as_mut().unwrap()
    }
}
impl Drop for RecordingFrame {
    fn drop(&mut self) {
        PREPARATION.with(|current| *current.borrow_mut() = self.previous.take());
    }
}
