use super::{
    Arc, BTreeMap, CancelToken, JoinHandle, Mutex, RefCell, Service, VecDeque, Wake, WorkFailure,
    mpsc,
};

type Run<R> = Box<dyn FnOnce(&CancelToken) -> R + Send>;
pub(super) struct Job<R> {
    pub generation: u64,
    pub cancel: CancelToken,
    pub run: Run<R>,
}
pub(super) struct Entry<R> {
    pub cancel: CancelToken,
    pub result: Option<Result<R, WorkFailure>>,
}
pub(super) struct State<R> {
    pub send: Option<mpsc::SyncSender<Job<R>>>,
    pub live: BTreeMap<u64, Entry<R>>,
    pub ready: VecDeque<u64>,
    pub next: u64,
    pub limit: usize,
}
pub(super) struct Shared<R> {
    pub state: Mutex<State<R>>,
    pub receive: Mutex<mpsc::Receiver<Job<R>>>,
    pub wake: Arc<Wake>,
}
pub(super) struct Control<R: Send + 'static> {
    pub shared: Arc<Shared<R>>,
    pub workers: RefCell<Vec<JoinHandle<()>>>,
}
impl<R: Send + 'static> Service for Control<R> {
    fn has_ready(&self) -> bool {
        !self.shared.state.lock().unwrap().ready.is_empty()
    }

    fn shutdown(&self) {
        let mut workers = self.workers.borrow_mut();
        let mut state = self.shared.state.lock().unwrap();
        for entry in state.live.values() {
            entry.cancel.cancel();
        }
        let send = state.send.take();
        let live = std::mem::take(&mut state.live);
        state.ready.clear();
        drop(state);
        drop(send);
        for worker in workers.drain(..) {
            worker
                .join()
                .expect("CPU worker panics are caught at the job boundary");
        }
        drop(live);
    }
}
impl<R: Send + 'static> Drop for Control<R> {
    fn drop(&mut self) {
        self.shutdown();
    }
}
pub(super) fn run<R: Send + 'static>(shared: &Shared<R>) {
    while let Some(job) = next(shared) {
        if job.cancel.is_cancelled() {
            continue;
        }
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (job.run)(&job.cancel)))
                .map_err(|_| WorkFailure::Panicked);
        let mut state = shared.state.lock().unwrap();
        if let Some(entry) = state.live.get_mut(&job.generation) {
            entry.result = Some(result);
            state.ready.push_back(job.generation);
            drop(state);
            shared.wake.notify();
        }
    }
}
fn next<R>(shared: &Shared<R>) -> Option<Job<R>> {
    // Release the receiver lock before executing work so all workers can run.
    shared.receive.lock().unwrap().recv().ok()
}
