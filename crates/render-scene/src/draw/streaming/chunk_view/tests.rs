use super::*;
fn oracle(
    cover: &[TerrainChunk],
    chunk: TerrainChunk,
    axis: usize,
    side: usize,
) -> Vec<TerrainChunkNeighbor> {
    let along = 1 - axis;
    let coordinate = [chunk.minimum[axis], chunk.maximum[axis]][side];
    let mut answer = Vec::<TerrainChunkNeighbor>::new();
    for interval in chunk.minimum[along]..chunk.maximum[along] {
        let owner = cover.iter().find(|other| {
            [other.maximum[axis], other.minimum[axis]][side] == coordinate
                && (other.minimum[along]..other.maximum[along]).contains(&interval)
        });
        if let Some(&owner) = owner {
            if let Some(last) = answer.last_mut()
                && last.chunk == owner
            {
                last.overlap[1] = interval + 1;
            } else {
                answer.push(TerrainChunkNeighbor {
                    chunk: owner,
                    overlap: [interval, interval + 1],
                });
            }
        }
    }
    answer
}

#[test]
fn adaptive_odd_rectangular_covers_match_unit_interval_owner_oracle() {
    let mut checked = 0;
    for seed in 0..48_u32 {
        let root = TerrainChunk {
            minimum: [0, 0],
            maximum: [19, 13],
            stride: 4,
        };
        let mut cover = vec![root];
        for turn in 0..36_u32 {
            let index = ((seed * 241 + turn * 17 + turn * turn) as usize) % cover.len();
            if let Some(children) = cover[index].children(2) {
                cover.splice(index..=index, children);
            }
        }
        let topology = Adjacency::new(cover.iter().copied());
        for &leaf in &cover {
            for (axis, side) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                assert_eq!(
                    topology.edge(leaf, axis, side),
                    oracle(&cover, leaf, axis, side)
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 1_000);
}

#[test]
fn one_edge_can_have_two_different_neighbor_strides() {
    let left = TerrainChunk {
        minimum: [0, 0],
        maximum: [4, 8],
        stride: 4,
    };
    let middle_bottom = TerrainChunk {
        minimum: [4, 0],
        maximum: [8, 4],
        stride: 2,
    };
    let middle_top = TerrainChunk {
        minimum: [4, 4],
        maximum: [8, 8],
        stride: 1,
    };
    let diagonal = TerrainChunk {
        minimum: [4, 8],
        maximum: [8, 12],
        stride: 1,
    };
    let topology = Adjacency::new([diagonal, middle_top, middle_bottom, left]);
    assert_eq!(
        topology.edge(left, 0, 1),
        [
            TerrainChunkNeighbor {
                chunk: middle_bottom,
                overlap: [0, 4]
            },
            TerrainChunkNeighbor {
                chunk: middle_top,
                overlap: [4, 8]
            }
        ]
    );
    assert_eq!(
        topology.edge(middle_top, 0, 0),
        [TerrainChunkNeighbor {
            chunk: left,
            overlap: [4, 8]
        }]
    );
    assert!(topology.edge(left, 0, 0).is_empty());
}
