//! Byte layout and validation of the temporal reconstruction uniform block.
use crate::{PostError, TEMPORAL_BLOCK_BYTES, TaaSettings, TemporalBlock};

const REJECTED: Result<(), PostError> = Err(PostError::UnsupportedProfile(
    "temporal reconstruction block",
));

fn block() -> TemporalBlock {
    TemporalBlock {
        render: [13, 7],
        output: [20, 11],
        jitter: [0.25, -0.375],
        phase_count: 8,
        settings: TaaSettings {
            history_weight: 0.75,
            clip_scale: 1.5,
        },
        history_valid: true,
    }
}

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[test]
fn the_block_is_three_sixteen_byte_rows() {
    assert_eq!(TEMPORAL_BLOCK_BYTES, 48);
    assert_eq!(TEMPORAL_BLOCK_BYTES % 16, 0);
    assert_eq!(block().to_bytes().unwrap().len(), 48);
}

#[test]
fn every_field_sits_at_its_documented_offset() {
    let bytes = block().to_bytes().unwrap();
    assert_eq!(
        floats(&bytes),
        [
            13.0, 7.0, 20.0, 11.0, 0.25, -0.375, 8.0, 0.75, 1.5, 1.0, 0.0, 0.0
        ]
    );
}

#[test]
fn a_reset_block_marks_the_history_invalid_and_changes_nothing_else() {
    let valid = block().to_bytes().unwrap();
    let reset = TemporalBlock {
        history_valid: false,
        ..block()
    }
    .to_bytes()
    .unwrap();
    let differing: Vec<usize> = (0..TEMPORAL_BLOCK_BYTES)
        .filter(|i| valid[*i] != reset[*i])
        .collect();
    assert!(differing.iter().all(|i| (36..40).contains(i)));
    assert_eq!(reset[36..40], 0.0_f32.to_le_bytes());
}

#[test]
fn the_native_taa_block_keeps_its_own_size() {
    assert_eq!(crate::TAA_UNIFORM_BYTES, 96);
    assert_ne!(crate::TAA_UNIFORM_BYTES, TEMPORAL_BLOCK_BYTES);
}

#[test]
fn a_valid_block_validates() {
    assert_eq!(block().validate(), Ok(()));
    let full = TemporalBlock {
        output: [1 << 24, 1],
        jitter: [1.0, -1.0],
        ..block()
    };
    assert_eq!(full.validate(), Ok(()));
}

#[test]
fn non_finite_or_out_of_range_values_are_rejected_before_any_byte_is_made() {
    let mut cases = Vec::new();
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.001, -2.0] {
        cases.push(TemporalBlock {
            jitter: [bad, 0.0],
            ..block()
        });
        cases.push(TemporalBlock {
            jitter: [0.0, bad],
            ..block()
        });
    }
    for bad in [f32::NAN, f32::INFINITY, -0.1, 0.995] {
        cases.push(TemporalBlock {
            settings: TaaSettings::new(bad),
            ..block()
        });
    }
    for bad in [f32::NAN, f32::INFINITY, 0.1, 3.0] {
        cases.push(TemporalBlock {
            settings: TaaSettings {
                clip_scale: bad,
                ..TaaSettings::DEFAULT
            },
            ..block()
        });
    }
    for render in [[0, 7], [13, 0], [(1 << 24) + 1, 7]] {
        cases.push(TemporalBlock { render, ..block() });
        cases.push(TemporalBlock {
            output: render,
            ..block()
        });
    }
    cases.push(TemporalBlock {
        phase_count: 0,
        ..block()
    });
    for case in cases {
        assert_eq!(case.validate(), REJECTED, "{case:?}");
        assert!(case.to_bytes().is_err(), "{case:?}");
    }
}
