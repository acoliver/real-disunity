//! Host-owned, bounded file reads. No foreign pointer is retained by the worker.
use std::collections::BTreeMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::JoinHandle;

#[derive(Clone, Copy)]
pub(crate) struct ReadLimits {
    pub requests: usize,
    pub bytes: usize,
    pub per_read: usize,
}

impl Default for ReadLimits {
    fn default() -> Self {
        Self {
            requests: 64,
            bytes: 64 * 1024 * 1024,
            per_read: 16 * 1024 * 1024,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReadStatus {
    InProgress = 0,
    Complete = 1,
    Failed = 2,
    Canceled = 3,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AdmissionError {
    InvalidRange,
    Capacity,
    Closed,
}

struct Budget {
    requests: usize,
    bytes: usize,
}

struct Reservation {
    budget: Arc<Mutex<Budget>>,
    bytes: usize,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut budget = self.budget.lock().expect("read budget panicked");
        budget.requests -= 1;
        budget.bytes -= self.bytes;
    }
}

struct State {
    status: ReadStatus,
    data: Vec<u8>,
    error: Option<String>,
    released: bool,
    reservation: Option<Reservation>,
}

pub(crate) struct ReadRequest {
    state: Mutex<State>,
    done: Condvar,
    canceled: AtomicBool,
}

impl ReadRequest {
    pub fn status(&self) -> ReadStatus {
        self.state.lock().expect("read state panicked").status
    }
    pub fn bytes_read(&self) -> usize {
        self.state.lock().expect("read state panicked").data.len()
    }
    pub fn error(&self) -> Option<String> {
        self.state
            .lock()
            .expect("read state panicked")
            .error
            .clone()
    }
    pub fn cancel(&self) {
        self.canceled.store(true, Ordering::Release);
    }

    pub fn complete(&self) {
        let mut state = self.state.lock().expect("read state panicked");
        while state.status == ReadStatus::InProgress {
            state = self.done.wait(state).expect("read state panicked");
        }
    }

    pub fn copy_data(&self, target: &mut [u8]) -> Result<usize, &'static str> {
        let state = self.state.lock().expect("read state panicked");
        if state.status == ReadStatus::InProgress {
            return Err("read is still in progress");
        }
        if target.len() < state.data.len() {
            return Err("read target is too small");
        }
        target[..state.data.len()].copy_from_slice(&state.data);
        Ok(state.data.len())
    }

    fn release(&self) {
        self.cancel();
        let mut state = self.state.lock().expect("read state panicked");
        state.released = true;
        if state.status != ReadStatus::InProgress {
            state.data = Vec::new();
            state.reservation = None;
        }
    }

    fn finish(&self, data: Vec<u8>, outcome: io::Result<ReadStatus>) {
        let mut state = self.state.lock().expect("read state panicked");
        match outcome {
            Ok(status) => state.status = status,
            Err(error) => {
                state.status = ReadStatus::Failed;
                state.error = Some(error.to_string());
            }
        }
        if state.released {
            drop(data);
            state.reservation = None;
        } else {
            state.data = data;
        }
        self.done.notify_all();
    }
}

struct Work {
    path: PathBuf,
    offset: u64,
    size: usize,
    request: Arc<ReadRequest>,
}

pub(crate) struct FileReads {
    limits: ReadLimits,
    budget: Arc<Mutex<Budget>>,
    entries: BTreeMap<u64, Arc<ReadRequest>>,
    next: u64,
    sender: Option<mpsc::SyncSender<Work>>,
    worker: Option<JoinHandle<()>>,
}

pub(crate) fn open_region(path: &Path, offset: u64) -> io::Result<Box<dyn Read + Send>> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "async reads require a regular file",
        ));
    }
    let mut file = std::fs::File::open(path)?;
    file.seek(SeekFrom::Start(offset))?;
    Ok(Box::new(file))
}

impl FileReads {
    pub fn new(limits: ReadLimits) -> io::Result<Self> {
        Self::with_opener(limits, open_region)
    }

    pub(super) fn with_opener(
        limits: ReadLimits,
        mut open: impl FnMut(&Path, u64) -> io::Result<Box<dyn Read + Send>> + Send + 'static,
    ) -> io::Result<Self> {
        if limits.requests == 0 || limits.per_read > limits.bytes || limits.per_read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid file-read limits",
            ));
        }
        let (sender, receiver) = mpsc::sync_channel::<Work>(limits.requests);
        let worker = std::thread::Builder::new()
            .name("disunity-file-reads".into())
            .spawn(move || {
                for work in receiver {
                    execute(work, &mut open);
                }
            })?;
        Ok(Self {
            limits,
            budget: Arc::new(Mutex::new(Budget {
                requests: 0,
                bytes: 0,
            })),
            entries: BTreeMap::new(),
            next: 1,
            sender: Some(sender),
            worker: Some(worker),
        })
    }

    pub fn submit(
        &mut self,
        path: PathBuf,
        offset: u64,
        size: usize,
    ) -> Result<u64, AdmissionError> {
        let sender = self.sender.as_ref().ok_or(AdmissionError::Closed)?;
        if path.as_os_str().is_empty()
            || size > self.limits.per_read
            || offset.checked_add(size as u64).is_none()
        {
            return Err(AdmissionError::InvalidRange);
        }
        let next = self
            .next
            .checked_add(1)
            .filter(|id| *id <= i64::MAX as u64)
            .ok_or(AdmissionError::Capacity)?;
        let reservation = {
            let mut budget = self.budget.lock().expect("read budget panicked");
            if budget.requests == self.limits.requests || size > self.limits.bytes - budget.bytes {
                return Err(AdmissionError::Capacity);
            }
            budget.requests += 1;
            budget.bytes += size;
            Reservation {
                budget: Arc::clone(&self.budget),
                bytes: size,
            }
        };
        let request = Arc::new(ReadRequest {
            state: Mutex::new(State {
                status: ReadStatus::InProgress,
                data: Vec::new(),
                error: None,
                released: false,
                reservation: Some(reservation),
            }),
            done: Condvar::new(),
            canceled: AtomicBool::new(false),
        });
        let work = Work {
            path,
            offset,
            size,
            request: Arc::clone(&request),
        };
        sender.try_send(work).map_err(|_| AdmissionError::Closed)?;
        let id = self.next;
        self.next = next;
        self.entries.insert(id, request);
        Ok(id)
    }

    pub fn request(&self, id: u64) -> Option<Arc<ReadRequest>> {
        self.entries.get(&id).cloned()
    }

    pub fn release(&mut self, id: u64) -> bool {
        if let Some(request) = self.entries.remove(&id) {
            request.release();
            true
        } else {
            false
        }
    }

    pub(super) fn close(&mut self) {
        for request in self.entries.values() {
            request.cancel();
        }
        self.sender = None;
    }
}

impl Drop for FileReads {
    fn drop(&mut self) {
        self.close();
        self.worker
            .take()
            .expect("file reader worker")
            .join()
            .expect("file reader panicked");
    }
}

fn execute(work: Work, open: &mut impl FnMut(&Path, u64) -> io::Result<Box<dyn Read + Send>>) {
    let mut data = Vec::new();
    let result = (|| {
        if work.request.canceled.load(Ordering::Acquire) {
            return Ok(ReadStatus::Canceled);
        }
        let mut file = open(&work.path, work.offset)?;
        data = vec![0; work.size];
        let mut done = 0;
        let result = read_chunks(&mut *file, &mut data, &mut done, &work.request.canceled);
        data.truncate(done);
        result
    })();
    work.request.finish(data, result);
}

fn read_chunks(
    file: &mut dyn Read,
    data: &mut [u8],
    done: &mut usize,
    canceled: &AtomicBool,
) -> io::Result<ReadStatus> {
    const CHUNK: usize = 64 * 1024;
    while *done < data.len() && !canceled.load(Ordering::Acquire) {
        let end = data.len().min(*done + CHUNK);
        match file.read(&mut data[*done..end]) {
            Ok(0) => {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "file region ended before requested size",
                ));
            }
            Ok(count) => *done += count,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
    // A full transfer wins over a cancellation arriving during the final read.
    Ok(if *done == data.len() {
        ReadStatus::Complete
    } else {
        ReadStatus::Canceled
    })
}
