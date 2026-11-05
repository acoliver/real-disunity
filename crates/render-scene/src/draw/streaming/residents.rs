use crate::terrain::streaming::TerrainChunk;

pub(super) fn insert<T>(residents: &mut Vec<(TerrainChunk, T)>, chunk: TerrainChunk, value: T) {
    let position = residents
        .binary_search_by_key(&chunk, |(key, _)| *key)
        .expect_err("terrain uploads must not duplicate resident chunks");
    residents.insert(position, (chunk, value));
}
