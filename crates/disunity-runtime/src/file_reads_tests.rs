use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use crate::file_reads::{AdmissionError, FileReads, ReadLimits, ReadStatus, open_region};

pub(crate) struct Fixture(pub PathBuf);

impl Fixture {
    pub fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("disunity-read-{}-{serial}", std::process::id()));
        let mut file = std::fs::File::create_new(&path).unwrap();
        std::io::Write::write_all(&mut file, b"0123456789abcdef").unwrap();
        Self(path)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_file(&self.0).unwrap();
    }
}

fn limits() -> ReadLimits {
    ReadLimits {
        requests: 2,
        bytes: 16,
        per_read: 12,
    }
}

struct Release(mpsc::Sender<()>);

impl Release {
    fn send(&self, value: ()) -> Result<(), mpsc::SendError<()>> {
        self.0.send(value)
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

type Gate = (FileReads, mpsc::Receiver<()>, Release);

fn gated() -> Gate {
    let (entered, observed) = mpsc::sync_channel(0);
    let (release, wait) = mpsc::channel();
    let mut first = true;
    let reads = FileReads::with_opener(limits(), move |path, offset| {
        if first {
            first = false;
            entered.send(()).unwrap();
            wait.recv().unwrap();
        }
        open_region(path, offset)
    })
    .unwrap();
    (reads, observed, Release(release))
}

#[test]
fn real_region_is_pending_until_released_then_owns_exact_bytes() {
    let fixture = Fixture::new();
    let (mut reads, entered, release) = gated();
    let id = reads.submit(fixture.0.clone(), 3, 5).unwrap();
    let request = reads.request(id).unwrap();
    entered.recv().unwrap();
    assert_eq!(request.status(), ReadStatus::InProgress);
    assert_eq!(request.bytes_read(), 0);
    assert!(request.copy_data(&mut [0xa5; 5]).is_err());
    release.send(()).unwrap();
    request.complete();
    assert_eq!(request.status(), ReadStatus::Complete);
    assert_eq!(request.bytes_read(), 5);
    assert_eq!(request.error(), None);
    let mut guarded = [0xa5; 9];
    assert_eq!(request.copy_data(&mut guarded[2..7]).unwrap(), 5);
    assert_eq!(&guarded, b"\xa5\xa534567\xa5\xa5");
    let mut short = [0xa5; 4];
    assert!(request.copy_data(&mut short).is_err());
    assert_eq!(short, [0xa5; 4]);
    request.cancel();
    assert_eq!(request.status(), ReadStatus::Complete);
    assert!(reads.release(id));
    assert!(reads.request(id).is_none());
    assert!(!reads.release(id));
}

#[test]
fn cancellation_is_acknowledged_and_admission_includes_disposed_inflight_storage() {
    let fixture = Fixture::new();
    let (mut reads, entered, release) = gated();
    let first = reads.submit(fixture.0.clone(), 0, 8).unwrap();
    entered.recv().unwrap();
    let second = reads
        .submit(fixture.0.with_extension("missing"), 8, 8)
        .unwrap();
    assert_ne!(first, second);
    let pending = reads.request(first).unwrap();
    let queued = reads.request(second).unwrap();
    queued.cancel();
    assert_eq!(queued.status(), ReadStatus::InProgress);
    assert_eq!(
        reads.submit(fixture.0.clone(), 0, 1),
        Err(AdmissionError::Capacity)
    );
    assert!(reads.release(first));
    assert_eq!(pending.status(), ReadStatus::InProgress);
    assert_eq!(
        reads.submit(fixture.0.clone(), 0, 1),
        Err(AdmissionError::Capacity)
    );
    release.send(()).unwrap();
    pending.complete();
    queued.complete();
    assert_eq!(pending.status(), ReadStatus::Canceled);
    assert_eq!(queued.status(), ReadStatus::Canceled);
    assert_eq!(queued.bytes_read(), 0);
    reads.release(second);
    let next = reads.submit(fixture.0.clone(), 0, 1).unwrap();
    assert!(next > second);
    reads.request(next).unwrap().complete();
}

#[test]
fn errors_short_reads_empty_reads_and_ranges_are_truthful() {
    let fixture = Fixture::new();
    let mut reads = FileReads::new(limits()).unwrap();
    for (path, offset, size, bytes) in [
        (fixture.0.with_extension("missing"), 0, 4, 0),
        (fixture.0.clone(), 14, 4, 2),
    ] {
        let id = reads.submit(path, offset, size).unwrap();
        let request = reads.request(id).unwrap();
        request.complete();
        assert_eq!(request.status(), ReadStatus::Failed);
        assert_eq!(request.bytes_read(), bytes);
        assert!(request.error().is_some());
        request.cancel();
        assert_eq!(request.status(), ReadStatus::Failed);
        reads.release(id);
    }
    let id = reads.submit(fixture.0.clone(), 16, 0).unwrap();
    let request = reads.request(id).unwrap();
    request.complete();
    assert_eq!(request.status(), ReadStatus::Complete);
    assert_eq!(request.copy_data(&mut []).unwrap(), 0);
    assert_eq!(
        reads.submit(fixture.0.clone(), u64::MAX, 2),
        Err(AdmissionError::InvalidRange)
    );
    assert_eq!(
        reads.submit(fixture.0.clone(), 0, 13),
        Err(AdmissionError::InvalidRange)
    );
    assert_eq!(
        reads.submit(PathBuf::new(), 0, 1),
        Err(AdmissionError::InvalidRange)
    );
}

struct CancelAfterChunk {
    source: Box<dyn Read + Send>,
    entered: mpsc::SyncSender<()>,
    wait: mpsc::Receiver<()>,
    first: bool,
}

impl Read for CancelAfterChunk {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.first {
            self.first = false;
            let count = self.source.read(&mut buffer[..2])?;
            self.entered.send(()).unwrap();
            self.wait.recv().unwrap();
            Ok(count)
        } else {
            self.source.read(buffer)
        }
    }
}

#[test]
fn cancellation_after_partial_io_keeps_the_actual_transferred_count() {
    let fixture = Fixture::new();
    let (entered, observed) = mpsc::sync_channel(0);
    let (release, wait) = mpsc::channel();
    let mut wait = Some(wait);
    let mut reads = FileReads::with_opener(limits(), move |path: &Path, offset| {
        Ok(Box::new(CancelAfterChunk {
            source: open_region(path, offset)?,
            entered: entered.clone(),
            wait: wait.take().unwrap(),
            first: true,
        }))
    })
    .unwrap();
    let release = Release(release);
    let id = reads.submit(fixture.0.clone(), 4, 8).unwrap();
    let request = reads.request(id).unwrap();
    observed.recv().unwrap();
    request.cancel();
    assert_eq!(request.status(), ReadStatus::InProgress);
    release.send(()).unwrap();
    request.complete();
    assert_eq!(request.status(), ReadStatus::Canceled);
    assert_eq!(request.bytes_read(), 2);
    let mut bytes = [0xa5; 2];
    request.copy_data(&mut bytes).unwrap();
    assert_eq!(&bytes, b"45");
}

#[test]
fn shutdown_cancels_queued_requests_and_joins_the_worker() {
    let fixture = Fixture::new();
    let (mut reads, entered, release) = gated();
    reads.submit(fixture.0.clone(), 0, 4).unwrap();
    entered.recv().unwrap();
    let second = reads.submit(fixture.0.clone(), 4, 4).unwrap();
    let pending = reads.request(second).unwrap();
    reads.close();
    assert_eq!(
        reads.submit(fixture.0.clone(), 0, 1),
        Err(AdmissionError::Closed)
    );
    release.send(()).unwrap();
    drop(reads);
    assert_eq!(pending.status(), ReadStatus::Canceled);
    assert_eq!(pending.bytes_read(), 0);
}

#[test]
fn byte_budget_and_zero_length_request_budget_are_independent() {
    let fixture = Fixture::new();
    let (mut reads, entered, release) = gated();
    let id = reads.submit(fixture.0.clone(), 0, 12).unwrap();
    entered.recv().unwrap();
    assert_eq!(
        reads.submit(fixture.0.clone(), 0, 5),
        Err(AdmissionError::Capacity)
    );
    release.send(()).unwrap();
    reads.request(id).unwrap().complete();
    assert_eq!(
        reads.submit(fixture.0.clone(), 0, 5),
        Err(AdmissionError::Capacity)
    );
    reads.release(id);
    for _ in 0..2 {
        let id = reads.submit(fixture.0.clone(), 0, 0).unwrap();
        reads.request(id).unwrap().complete();
    }
    assert_eq!(
        reads.submit(fixture.0.clone(), 0, 0),
        Err(AdmissionError::Capacity)
    );
}

#[test]
fn a_canceled_empty_request_does_not_claim_the_unopened_file_completed() {
    let fixture = Fixture::new();
    let (mut reads, entered, release) = gated();
    reads.submit(fixture.0.clone(), 0, 1).unwrap();
    entered.recv().unwrap();
    let id = reads
        .submit(fixture.0.with_extension("missing"), 0, 0)
        .unwrap();
    let request = reads.request(id).unwrap();
    request.cancel();
    release.send(()).unwrap();
    request.complete();
    assert_eq!(request.status(), ReadStatus::Canceled);
    assert_eq!(request.bytes_read(), 0);
}

#[test]
fn a_full_transfer_wins_when_canceled_inside_the_final_os_read() {
    let fixture = Fixture::new();
    let (entered, observed) = mpsc::sync_channel(0);
    let (release, wait) = mpsc::channel();
    let mut wait = Some(wait);
    let mut reads = FileReads::with_opener(limits(), move |path, offset| {
        Ok(Box::new(CancelAfterChunk {
            source: open_region(path, offset)?,
            entered: entered.clone(),
            wait: wait.take().unwrap(),
            first: true,
        }))
    })
    .unwrap();
    let release = Release(release);
    let id = reads.submit(fixture.0.clone(), 0, 2).unwrap();
    let request = reads.request(id).unwrap();
    observed.recv().unwrap();
    request.cancel();
    release.send(()).unwrap();
    request.complete();
    assert_eq!(request.status(), ReadStatus::Complete);
    assert_eq!(request.bytes_read(), 2);
}
