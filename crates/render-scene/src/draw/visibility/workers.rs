//! Dividing one frame's selection among threads.
//!
//! A view's selection is a walk over each batch, and views differ greatly in how much of it
//! they do: a cascade may keep several times the placements the camera does. Splitting the
//! frame by view leaves the other threads idle while the slowest view finishes, so the frame is
//! divided into chunks of about equal work instead, each a run of one view's batches, and the
//! threads take chunks until none remain.
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Threads that select a frame at most. The walks share memory and the machine's other work, so
/// each thread beyond the first adds less: measured on a busy sixteen-thread machine, a frame
/// of four views took 10.7 ms on 2 threads, 6.6 on 4, 5.2 on 6 and 4.5 on 8.
const MAX_THREADS: usize = 8;

/// Placements of work, across every view, for each thread beyond the first. A thread costs
/// tens of microseconds to start and join, more when the machine is busy, and a walk spends
/// 50 to 90 nanoseconds a placement, so a smaller frame selects faster with fewer threads.
const WORK_PER_THREAD: usize = 2048;

/// Placements of work below which a chunk is not worth its own selection to fill and append.
const MIN_CHUNK_WORK: usize = 256;

/// Chunks per thread. A view's chunks differ several times in cost for equal work, because a
/// walk tests far fewer placements where it rejects whole groups than where it admits them, so
/// the chunks are small enough that a thread finishing early finds many left to take.
const CHUNKS_PER_THREAD: usize = 64;

/// The threads a frame may select on: the machine's parallelism, within `MAX_THREADS`.
pub(in crate::draw) fn available() -> usize {
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(MAX_THREADS)
}

/// A run of one view's batches that a single thread selects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Chunk {
    pub view: usize,
    pub batches: Range<usize>,
}

/// How many of `threads` threads a frame of `work` placements, summed over its views, uses.
pub(super) fn threads_for(work: usize, threads: usize) -> usize {
    (work / WORK_PER_THREAD).clamp(1, threads.max(1))
}

/// Splits each of `views` views over batches with the given `batch_work` into chunks of about
/// equal work for `threads` threads, in view order and then batch order. A chunk ends with the
/// batch that fills its share, so it holds at most one batch's work more than the share.
pub(super) fn chunks(batch_work: &[usize], views: usize, threads: usize) -> Vec<Chunk> {
    let total: usize = batch_work.iter().sum::<usize>() * views;
    let share = if threads <= 1 {
        usize::MAX
    } else {
        total
            .div_ceil(threads * CHUNKS_PER_THREAD)
            .max(MIN_CHUNK_WORK)
    };
    let mut chunks = Vec::new();
    for view in 0..views {
        let mut start = 0;
        let mut taken = 0;
        for (batch, work) in batch_work.iter().enumerate() {
            taken += work;
            if taken >= share {
                chunks.push(Chunk {
                    view,
                    batches: start..batch + 1,
                });
                (start, taken) = (batch + 1, 0);
            }
        }
        if start < batch_work.len() {
            chunks.push(Chunk {
                view,
                batches: start..batch_work.len(),
            });
        }
    }
    chunks
}

/// Runs `job` for each index below `count` on `threads` threads, the caller's among them, and
/// returns the results in index order. Each thread takes the next index not yet taken, so the
/// results do not depend on which thread ran which job.
pub(super) fn run<T: Send>(
    count: usize,
    threads: usize,
    job: impl Fn(usize) -> T + Sync,
) -> Vec<T> {
    let next = AtomicUsize::new(0);
    let take = || {
        let mut done = Vec::new();
        loop {
            let index = next.fetch_add(1, Ordering::Relaxed);
            if index >= count {
                return done;
            }
            done.push((index, job(index)));
        }
    };
    let mut done = std::thread::scope(|scope| {
        let helpers: Vec<_> = (1..threads.min(count)).map(|_| scope.spawn(take)).collect();
        let mut done = take();
        for helper in helpers {
            done.extend(
                helper
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
            );
        }
        done
    });
    done.sort_unstable_by_key(|(index, _)| *index);
    done.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every view's chunks, in order, cover its batches exactly once.
    fn assert_covers(chunks: &[Chunk], views: usize, batches: usize) {
        for view in 0..views {
            let mut next = 0;
            for chunk in chunks.iter().filter(|chunk| chunk.view == view) {
                assert_eq!(
                    chunk.batches.start, next,
                    "chunks of view {view} are contiguous"
                );
                assert!(!chunk.batches.is_empty(), "no chunk is empty");
                next = chunk.batches.end;
            }
            assert_eq!(next, batches, "view {view} is covered to its last batch");
        }
        let order: Vec<usize> = chunks.iter().map(|chunk| chunk.view).collect();
        assert!(order.is_sorted(), "chunks follow the order of the views");
    }

    #[test]
    fn one_thread_takes_each_view_whole() {
        let chunks = chunks(&[5, 0, 9, 1], 3, 1);
        assert_eq!(
            chunks,
            (0..3)
                .map(|view| Chunk {
                    view,
                    batches: 0..4
                })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn many_threads_cover_every_batch_of_every_view_once() {
        let work: Vec<usize> = (0..57).map(|batch| batch * 37 % 101).collect();
        for threads in [2, 3, 4, 8] {
            let chunks = chunks(&work, 5, threads);
            assert_covers(&chunks, 5, work.len());
            assert!(chunks.len() > 5, "{threads} threads split the views");
        }
    }

    #[test]
    fn chunks_hold_about_equal_work() {
        let work = vec![10; 4000];
        let chunks = chunks(&work, 4, 4);
        let share = (4 * 40_000_usize)
            .div_ceil(4 * CHUNKS_PER_THREAD)
            .max(MIN_CHUNK_WORK);
        for view in 0..4 {
            let sizes: Vec<usize> = chunks
                .iter()
                .filter(|chunk| chunk.view == view)
                .map(|chunk| chunk.batches.clone().map(|batch| work[batch]).sum())
                .collect();
            let (last, full) = sizes.split_last().unwrap();
            assert!(
                full.iter().all(|size| (share..share + 10).contains(size)),
                "{sizes:?}"
            );
            assert!(*last <= share + 10);
        }
    }

    #[test]
    fn a_batch_larger_than_the_share_ends_its_chunk() {
        let work = [1, 1, 100_000, 1, 1];
        let chunks = chunks(&work, 1, 4);
        assert_covers(&chunks, 1, work.len());
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].batches, 0..3);
        assert_eq!(chunks[1].batches, 3..5);
    }

    #[test]
    fn nothing_to_select_makes_no_chunks() {
        assert!(chunks(&[], 5, 4).is_empty());
        assert!(chunks(&[3, 4], 0, 4).is_empty());
    }

    #[test]
    fn batches_without_work_still_belong_to_a_chunk() {
        let chunks = chunks(&[0, 0, 0], 2, 4);
        assert_covers(&chunks, 2, 3);
    }

    #[test]
    fn a_small_frame_uses_one_thread_and_a_large_one_as_many_as_allowed() {
        assert_eq!(threads_for(0, 4), 1);
        assert_eq!(threads_for(WORK_PER_THREAD - 1, 4), 1);
        assert_eq!(threads_for(WORK_PER_THREAD * 2, 4), 2);
        assert_eq!(threads_for(WORK_PER_THREAD * 100, 4), 4);
        assert_eq!(threads_for(WORK_PER_THREAD * 100, 64), 64);
        assert_eq!(threads_for(WORK_PER_THREAD * 100, 0), 1);
    }

    #[test]
    fn results_come_back_in_job_order_on_any_number_of_threads() {
        for threads in [1, 2, 3, 8, 50] {
            for count in [0, 1, 2, 7, 100] {
                let results = run(count, threads, |index| index * index);
                assert_eq!(
                    results,
                    (0..count).map(|index| index * index).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn every_job_runs_once_across_threads() {
        let runs: Vec<AtomicUsize> = (0..64).map(|_| AtomicUsize::new(0)).collect();
        run(runs.len(), 4, |index| {
            runs[index].fetch_add(1, Ordering::Relaxed)
        });
        assert!(runs.iter().all(|runs| runs.load(Ordering::Relaxed) == 1));
    }
}
