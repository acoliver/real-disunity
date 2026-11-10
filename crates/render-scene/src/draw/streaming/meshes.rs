//! Builds streaming terrain chunk meshes on a worker thread, so a frame only uploads meshes
//! that are finished. The frame never waits for a build.
use crate::terrain::{
    TerrainError, TerrainMesh, chunks,
    streaming::{TerrainChunk, TerrainSelection},
};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, mpsc},
    thread::JoinHandle,
};
use worldrunner_render_plan::SceneTerrain;
use worldrunner_render_post::FrameError;

/// A chunk's mesh, or the error `chunks::build` reported for it. A failed build counts as
/// finished, so the frame that admits the chunk reports the error as it did before the worker.
pub(super) type Built = Result<TerrainMesh, TerrainError>;
type Finished = (TerrainChunk, Built);

#[cfg(test)]
thread_local! {
    static AWAIT_BUILDS: std::cell::Cell<bool> = const { std::cell::Cell::new(true) };
}

/// Makes frames on this thread wait for the builds their selection asks for, so a chunk selected
/// in a frame uploads in that frame. Returns the previous setting. Tests inside this crate wait
/// by default; production frames never do.
#[cfg(test)]
pub(crate) fn await_builds(enabled: bool) -> bool {
    AWAIT_BUILDS.replace(enabled)
}

#[cfg(test)]
pub(super) fn awaiting_builds() -> bool {
    AWAIT_BUILDS.get()
}

#[derive(Debug, Default)]
struct Queue {
    /// Chunks to build in priority order. Each frame replaces them with what it still wants.
    pending: VecDeque<TerrainChunk>,
    active: Option<TerrainChunk>,
    closed: bool,
}

#[derive(Debug, Default)]
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}
impl Shared {
    // The queue holds plain data that every update leaves consistent, so a poisoned lock is
    // still usable.
    fn lock(&self) -> MutexGuard<'_, Queue> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn next(&self) -> Option<TerrainChunk> {
        let mut queue = self.lock();
        loop {
            if queue.closed {
                return None;
            }
            if let Some(chunk) = queue.pending.pop_front() {
                queue.active = Some(chunk);
                return Some(chunk);
            }
            queue = self
                .wake
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Delivers a result while the chunk is still marked active, so a frame never sees a chunk
    /// that is neither building nor finished. False when the frame side is gone.
    fn finish(&self, done: &mpsc::Sender<Finished>, finished: Finished) -> bool {
        let mut queue = self.lock();
        queue.active = None;
        let delivered = done.send(finished).is_ok();
        drop(queue);
        self.wake.notify_all();
        delivered
    }

    fn close(&self) {
        self.lock().closed = true;
        self.wake.notify_all();
    }
}

/// Closes the queue when the worker leaves for any reason, a panic included.
struct CloseOnExit<'a>(&'a Shared);
impl Drop for CloseOnExit<'_> {
    fn drop(&mut self) {
        self.0.close();
    }
}

fn run(terrain: &SceneTerrain, shared: &Shared, done: &mpsc::Sender<Finished>) {
    let _close = CloseOnExit(shared);
    while let Some(chunk) = shared.next() {
        if !shared.finish(done, (chunk, chunks::build(terrain, chunk))) {
            return;
        }
    }
}

#[derive(Debug)]
struct Worker {
    shared: Arc<Shared>,
    done: mpsc::Receiver<Finished>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    fn spawn(terrain: Arc<SceneTerrain>) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let (send, done) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("terrain-chunks".into())
            .spawn({
                let shared = shared.clone();
                move || run(&terrain, &shared, &send)
            })?;
        Ok(Self {
            shared,
            done,
            thread: Some(thread),
        })
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shared.close();
        if let Some(thread) = self.thread.take() {
            // A panic already closed the queue and surfaced as a stopped worker. Dropping
            // must not panic again.
            let _ = thread.join();
        }
    }
}

fn stopped(reason: String) -> FrameError {
    FrameError::FeaturePreparation {
        feature: "terrain streaming",
        reason,
    }
}

/// The frame side of the mesh worker. The worker starts with the first request. Dropping this
/// closes the queue and joins the worker, which finishes at most the build in progress.
///
/// Outstanding builds never exceed `limit` queued plus the one in progress. The finished
/// store holds the chunks the last selection wanted, plus results that arrived since.
#[derive(Debug)]
pub(in crate::draw) struct MeshBuilds {
    terrain: Arc<SceneTerrain>,
    limit: usize,
    worker: Option<Worker>,
    finished: BTreeMap<TerrainChunk, Built>,
    /// The camera selection's last requests, in priority order.
    camera: Vec<TerrainChunk>,
    /// Chunks the shadow casters need, requested after the camera's.
    casters: Vec<TerrainChunk>,
}
impl MeshBuilds {
    /// A small multiple of the upload budget keeps the worker busy across a frame without
    /// letting stale requests pile up.
    pub fn new(terrain: Arc<SceneTerrain>, uploads_per_frame: u32) -> Self {
        Self {
            terrain,
            limit: 2 * uploads_per_frame as usize,
            worker: None,
            finished: BTreeMap::new(),
            camera: Vec::new(),
            casters: Vec::new(),
        }
    }

    pub fn is_ready(&self, chunk: TerrainChunk) -> bool {
        self.finished.contains_key(&chunk)
    }

    pub fn get(&self, chunk: TerrainChunk) -> Option<&Built> {
        self.finished.get(&chunk)
    }

    /// Takes in the meshes the worker finished since the last frame.
    pub fn collect(&mut self) -> Result<(), FrameError> {
        let Some(worker) = &self.worker else {
            return Ok(());
        };
        loop {
            match worker.done.try_recv() {
                Ok((chunk, built)) => {
                    self.finished.insert(chunk, built);
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(stopped("the chunk mesh worker stopped".into()));
                }
            }
        }
    }

    /// Keeps the meshes the selection and the shadow casters still need and replaces the
    /// worker's queue with the selection's unready chunks, in its priority order, then its
    /// prefetch, then the casters' requests.
    pub fn update(&mut self, selection: &TerrainSelection) -> Result<(), FrameError> {
        let wanted: BTreeSet<_> = selection
            .uploads
            .iter()
            .chain(&selection.held)
            .chain(&selection.unready)
            .chain(&selection.prefetch)
            .chain(&self.casters)
            .copied()
            .collect();
        self.finished.retain(|chunk, _| wanted.contains(chunk));
        self.camera = selection
            .unready
            .iter()
            .chain(&selection.prefetch)
            .copied()
            .collect();
        self.requeue()
    }

    /// Replaces the shadow casters' requests. Built meshes of earlier requests stay until the
    /// next [`Self::update`].
    pub fn request_casters(&mut self, chunks: Vec<TerrainChunk>) -> Result<(), FrameError> {
        if chunks == self.casters {
            return Ok(());
        }
        self.casters = chunks;
        self.requeue()
    }

    fn requeue(&mut self) -> Result<(), FrameError> {
        if self.camera.is_empty() && self.casters.is_empty() && self.worker.is_none() {
            return Ok(());
        }
        let limit = self.limit;
        let requests: Vec<_> = self
            .camera
            .iter()
            .chain(&self.casters)
            .copied()
            .filter(|chunk| !self.finished.contains_key(chunk))
            .collect();
        let worker = self.worker()?;
        let mut queue = worker.shared.lock();
        let next: Vec<_> = requests
            .into_iter()
            .filter(|chunk| queue.active != Some(*chunk))
            .take(limit)
            .collect();
        queue.pending = next.into();
        drop(queue);
        worker.shared.wake.notify_all();
        Ok(())
    }

    fn worker(&mut self) -> Result<&Worker, FrameError> {
        if self.worker.is_none() {
            let worker = Worker::spawn(self.terrain.clone()).map_err(|error| {
                stopped(format!("could not start the chunk mesh worker: {error}"))
            })?;
            self.worker = Some(worker);
        }
        Ok(self.worker.as_ref().expect("worker"))
    }

    /// Blocks until every requested build finished or the worker stopped.
    #[cfg(test)]
    pub fn wait_idle(&self) {
        let Some(worker) = &self.worker else {
            return;
        };
        let mut queue = worker.shared.lock();
        while !(queue.closed || queue.pending.is_empty() && queue.active.is_none()) {
            queue = worker
                .shared
                .wake
                .wait(queue)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// Builds that are queued or in progress.
    #[cfg(test)]
    pub fn outstanding(&self) -> usize {
        self.worker.as_ref().map_or(0, |worker| {
            let queue = worker.shared.lock();
            queue.pending.len() + usize::from(queue.active.is_some())
        })
    }

    #[cfg(test)]
    pub fn finished_count(&self) -> usize {
        self.finished.len()
    }

    /// The most chunks queued at once.
    #[cfg(test)]
    pub fn limit(&self) -> usize {
        self.limit
    }
}
