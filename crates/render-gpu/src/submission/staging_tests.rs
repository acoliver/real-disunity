use super::{IDLE_TRANSACTIONS, MINIMUM_CHUNK_BYTES, MapState, Pool, Reservation};

/// A pool over numbered fake buffers; `created` lists the sizes asked for and `limit` is the
/// device's `max_buffer_size`.
struct Fixture {
    pool: Pool<u32>,
    created: Vec<u64>,
    limit: u64,
}
impl Fixture {
    fn new() -> Self {
        Self::limited(wgpu::Limits::default().max_buffer_size)
    }
    fn limited(limit: u64) -> Self {
        Self {
            pool: Pool::default(),
            created: Vec::new(),
            limit,
        }
    }
    fn reserve(&mut self, transaction: u64, len: u64) -> Reservation<u32> {
        let created = &mut self.created;
        let limit = self.limit;
        self.pool.reserve(
            transaction,
            len,
            || limit,
            |size| {
                created.push(size);
                u32::try_from(created.len()).unwrap()
            },
        )
    }
    fn submit(&mut self, transaction: u64) -> Vec<MapState> {
        self.pool
            .submit(transaction)
            .into_iter()
            .map(|(_, state)| state)
            .collect()
    }
}

fn at(buffer: u32, offset: u64) -> Reservation<u32> {
    Reservation { buffer, offset }
}

#[test]
fn a_transaction_packs_uploads_into_one_aligned_chunk() {
    let mut fixture = Fixture::new();
    assert_eq!(fixture.reserve(1, 12), at(1, 0));
    assert_eq!(fixture.reserve(1, 4), at(1, 16));
    assert_eq!(fixture.reserve(1, 64), at(1, 24));
    assert_eq!(fixture.created, [MINIMUM_CHUNK_BYTES]);
    let statistics = fixture.pool.statistics();
    assert_eq!((statistics.uploads, statistics.uploaded_bytes), (3, 80));
    assert_eq!((statistics.chunks, statistics.chunks_created), (1, 1));
}

#[test]
fn a_submitted_chunk_is_not_reused_before_its_mapping_completes() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 16);
    let states = fixture.submit(1);
    assert_eq!(states.len(), 1);
    assert_eq!(fixture.reserve(2, 16), at(2, 0));
    assert_eq!(fixture.created.len(), 2);
}

#[test]
fn a_chunk_is_reused_from_its_start_once_its_mapping_completes() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 16);
    fixture.reserve(1, 16);
    for state in fixture.submit(1) {
        state.complete(true);
    }
    assert_eq!(fixture.reserve(2, 32), at(1, 0));
    assert_eq!(fixture.created.len(), 1);
    assert_eq!(fixture.pool.statistics().chunks_created, 1);
}

#[test]
fn steady_frames_in_flight_settle_on_a_fixed_set_of_chunks() {
    let mut fixture = Fixture::new();
    let mut in_flight = std::collections::VecDeque::new();
    for transaction in 1..=64 {
        for _ in 0..40 {
            fixture.reserve(transaction, 80);
        }
        in_flight.push_back(fixture.submit(transaction));
        // Three frames in flight: the oldest completes before the next one records.
        if in_flight.len() == 3 {
            for state in in_flight.pop_front().unwrap() {
                state.complete(true);
            }
        }
    }
    assert_eq!(fixture.created.len(), 3);
}

#[test]
fn uploads_larger_than_a_chunk_get_the_next_power_of_two() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 16);
    let large = MINIMUM_CHUNK_BYTES + 4;
    assert_eq!(fixture.reserve(1, large), at(2, 0));
    // The remainder of the first chunk is still used for small uploads.
    assert_eq!(fixture.reserve(1, 16), at(1, 16));
    assert_eq!(
        fixture.created,
        [MINIMUM_CHUNK_BYTES, 2 * MINIMUM_CHUNK_BYTES]
    );
    let statistics = fixture.pool.statistics();
    assert_eq!(statistics.bytes, 3 * MINIMUM_CHUNK_BYTES);
    assert_eq!(statistics.bytes_created, 3 * MINIMUM_CHUNK_BYTES);
}

#[test]
fn a_full_chunk_grows_the_pool_and_free_chunks_are_chosen_smallest_first() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, MINIMUM_CHUNK_BYTES);
    assert_eq!(fixture.reserve(1, 8), at(2, 0));
    fixture.reserve(1, 4 * MINIMUM_CHUNK_BYTES);
    for state in fixture.submit(1) {
        state.complete(true);
    }
    assert_eq!(fixture.reserve(2, 8), at(1, 0));
    assert_eq!(fixture.reserve(2, 2 * MINIMUM_CHUNK_BYTES), at(3, 0));
    assert_eq!(fixture.created.len(), 3);
}

#[test]
fn a_cancelled_transaction_returns_its_chunks_for_immediate_reuse() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 64);
    fixture.pool.cancel(1);
    // The cancelled bytes are never copied: the next transaction writes over them from the start.
    assert_eq!(fixture.reserve(2, 64), at(1, 0));
    assert!(fixture.submit(1).is_empty());
    assert_eq!(fixture.created.len(), 1);
}

#[test]
fn transactions_never_share_a_recording_chunk() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 16);
    assert_eq!(fixture.reserve(2, 16), at(2, 0));
    assert_eq!(fixture.submit(1).len(), 1);
    assert_eq!(fixture.submit(2).len(), 1);
}

#[test]
fn a_failed_mapping_drops_the_chunk() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 16);
    for state in fixture.submit(1) {
        state.complete(false);
    }
    assert_eq!(fixture.reserve(2, 16), at(2, 0));
    assert_eq!(fixture.pool.statistics().chunks, 1);
}

#[test]
fn idle_free_chunks_are_released() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 16);
    fixture.reserve(1, MINIMUM_CHUNK_BYTES);
    for state in fixture.submit(1) {
        state.complete(true);
    }
    // Only the small chunk keeps being used.
    for transaction in 2..IDLE_TRANSACTIONS + 3 {
        fixture.reserve(transaction, 16);
        for state in fixture.submit(transaction) {
            state.complete(true);
        }
    }
    assert_eq!(fixture.pool.statistics().chunks, 1);
    assert_eq!(fixture.created.len(), 2);
}

#[test]
#[should_panic(expected = "nonzero multiple of 4")]
fn unaligned_uploads_fail_fast() {
    Fixture::new().reserve(1, 6);
}

#[test]
#[should_panic(expected = "nonzero multiple of 4")]
fn empty_uploads_fail_fast() {
    Fixture::new().reserve(1, 0);
}

#[test]
#[should_panic(expected = "recording transaction")]
fn staging_outside_a_transaction_fails_fast() {
    Fixture::new().reserve(0, 4);
}

#[test]
fn a_chunk_is_never_larger_than_the_device_allows() {
    // Below the minimum chunk: even the smallest upload gets a chunk of the whole limit.
    let mut fixture = Fixture::limited(128 * 1024);
    assert_eq!(fixture.reserve(1, 4), at(1, 0));
    assert_eq!(fixture.created, [128 * 1024]);
    // Between two powers of two, and not a multiple of the copy alignment.
    for limit in [196_612, 196_614] {
        let mut fixture = Fixture::limited(limit);
        fixture.reserve(1, 4);
        assert_eq!(fixture.created, [196_612]);
    }
}

#[test]
fn growth_stops_at_the_device_limit_and_an_upload_of_the_whole_limit_fits() {
    let limit = 196_612;
    let mut fixture = Fixture::limited(limit);
    let mut len = 4;
    while len < limit {
        fixture.reserve(1, len);
        len *= 2;
    }
    fixture.reserve(1, limit);
    // 4 to 65,536 bytes share the first chunk; 131,072 and 196,612 each need another.
    assert_eq!(fixture.created, [limit, limit, limit]);
    for state in fixture.submit(1) {
        state.complete(true);
    }
    assert_eq!(fixture.reserve(2, limit), at(1, 0));
    assert_eq!(fixture.created.len(), 3);
}

#[test]
fn the_device_limit_is_read_only_when_a_chunk_is_created() {
    let mut fixture = Fixture::new();
    fixture.reserve(1, 16);
    let created = fixture.pool.reserve(
        1,
        16,
        || panic!("a chunk with room needs no limit"),
        |_| unreachable!("the recording chunk has room"),
    );
    assert_eq!(created, at(1, 16));
}

#[test]
#[should_panic(
    expected = "a staged upload of 196616 bytes exceeds the device's max_buffer_size of 196612 bytes"
)]
fn an_upload_larger_than_the_device_allows_fails_fast() {
    Fixture::limited(196_612).reserve(1, 196_616);
}
