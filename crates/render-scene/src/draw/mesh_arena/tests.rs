use super::plan;

#[test]
fn levels_pack_back_to_back_and_the_last_chunk_is_exactly_full() {
    let packed = plan(&[56, 112, 56], 1 << 20);
    assert_eq!(packed.places, [(0, 0), (0, 56), (0, 168)]);
    assert_eq!(packed.chunk_bytes, [224]);
}

#[test]
fn a_level_that_would_pass_the_limit_starts_the_next_chunk() {
    let packed = plan(&[60, 30, 30, 50], 100);
    assert_eq!(packed.places, [(0, 0), (0, 60), (1, 0), (1, 30)]);
    assert_eq!(packed.chunk_bytes, [90, 80]);
}

#[test]
fn a_level_larger_than_the_limit_has_a_chunk_of_its_own() {
    let packed = plan(&[10, 500, 10], 100);
    assert_eq!(packed.places, [(0, 0), (1, 0), (2, 0)]);
    assert_eq!(packed.chunk_bytes, [10, 500, 10]);
}

#[test]
fn an_empty_level_joins_the_chunk_it_follows() {
    let packed = plan(&[500, 0, 8], 100);
    assert_eq!(packed.places, [(0, 0), (0, 500), (1, 0)]);
    assert_eq!(packed.chunk_bytes, [500, 8]);
}

#[test]
fn a_limit_of_one_gives_every_level_a_chunk_of_its_own() {
    let packed = plan(&[8, 8, 8], 1);
    assert_eq!(packed.places, [(0, 0), (1, 0), (2, 0)]);
}

#[test]
fn no_levels_make_no_chunks() {
    let packed = plan(&[], 100);
    assert!(packed.places.is_empty());
    assert!(packed.chunk_bytes.is_empty());
}
