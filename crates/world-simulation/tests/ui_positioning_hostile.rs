#![cfg(feature = "ui")]
#[path = "ui_font/hostile_positioning.rs"]
mod hostile;
#[path = "ui_font/positioning.rs"]
mod positioning;
mod ui_font;

use positioning::{Lookup, default_gpos, font, kern, words};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use worldrunner_world_simulation::ui::{FaceBook, FontBook, TextRun, UiError, UiLimits};

#[test]
fn aliased_pair_slots_have_an_exact_retained_bound() {
    let bytes = font(Some(hostile::alias_gpos(32)), None);
    let mut faces = FaceBook::default();
    let id = faces.load(bytes, 0, 20.).unwrap();
    assert_eq!(
        faces.pair_adjustments(id, &[1, 2]).unwrap()[0]
            .x_advance
            .to_bits(),
        (-64_f32).to_bits()
    );
    rejects(hostile::alias_gpos(33), UiError::PositioningLimit);
}

#[test]
fn expanded_work_counts_aliases_even_in_unsupported_lookups() {
    let mut table = hostile::alias_gpos(128);
    let list = usize::from(u16::from_be_bytes([table[8], table[9]]));
    let lookup = list + usize::from(u16::from_be_bytes([table[list + 2], table[list + 3]]));
    table[lookup..lookup + 2].copy_from_slice(&1_u16.to_be_bytes());
    rejects(table, UiError::PositioningLimit);
}

#[test]
fn malformed_top_level_gpos_does_not_enable_legacy() {
    rejects(words(&[2, 0, 10, 0, 0]), UiError::InvalidGpos);
}

fn rejects(table: Vec<u8>, error: UiError) {
    let bytes = font(Some(table), Some(kern(&[(1, 2, -50)])));
    let mut fonts = FontBook::new(UiLimits::default()).unwrap();
    assert_eq!(fonts.register(bytes.clone()), Err(error));
    assert_eq!(FaceBook::default().load(bytes, 0, 20.), Err(error));
}

#[test]
fn plain_font_without_gpos_has_no_pairs() {
    let mut faces = FaceBook::default();
    let id = faces.load(ui_font::font(), 0, 20.).unwrap();
    assert!(faces.pair_adjustments(id, &[1]).unwrap().is_empty());
}

#[test]
fn recursive_extension_is_rejected_in_a_bounded_subprocess() {
    for path in ["register", "load"] {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "extension_rejection_child", "--nocapture"])
            .env("GPOS_EXTENSION_CHILD", path)
            .stdin(Stdio::null())
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(
                    status.success(),
                    "extension {path} subprocess failed: {status}"
                );
                break;
            }
            if start.elapsed() >= Duration::from_secs(10) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("extension {path} subprocess exceeded hard timeout");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

#[test]
fn extension_rejection_child() {
    let Ok(path) = std::env::var("GPOS_EXTENSION_CHILD") else {
        return;
    };
    let lookup = Lookup {
        kind: 9,
        subtables: vec![words(&[1, 9, 0, 0])],
    };
    let bytes = font(Some(default_gpos(&[lookup])), None);
    if path == "register" {
        let mut fonts = FontBook::new(UiLimits::default()).unwrap();
        match fonts.register(bytes) {
            Err(error) => assert_eq!(error, UiError::InvalidGposExtension),
            Ok(id) => {
                let run = TextRun {
                    text: "AV".into(),
                    ..TextRun::new(id)
                };
                let _ = fonts.layout(&run, glam::Vec2::splat(100.));
                panic!("registration accepted a recursive extension");
            }
        }
    } else {
        let mut faces = FaceBook::default();
        match faces.load(bytes, 0, 20.) {
            Err(error) => assert_eq!(error, UiError::InvalidGposExtension),
            Ok(id) => {
                let _ = faces.pair_adjustments(id, &[1, 2]);
                panic!("face loading accepted a recursive extension");
            }
        }
    }
}

#[test]
fn malformed_extensions_are_rejected_without_legacy_fallback() {
    for data in [
        words(&[2, 2, 0, 8]),
        words(&[1, 2, 0, 0]),
        words(&[1, 2, 65535, 65535]),
        words(&[1, 9, 0, 8]),
        words(&[1, 2]),
    ] {
        rejects(
            default_gpos(&[Lookup {
                kind: 9,
                subtables: vec![data],
            }]),
            UiError::InvalidGposExtension,
        );
    }
}

#[test]
fn aliased_lookup_expansion_is_rejected_before_visiting_subtables() {
    let mut table = hostile::alias_gpos(8192);
    rejects(table.clone(), UiError::PositioningLimit);
    // Poison the shared lookup as an invalid extension. The work limit must win before
    // expanded traversal would discover it, proving aliases were charged in preflight.
    let list = usize::from(u16::from_be_bytes([table[8], table[9]]));
    let lookup = list + usize::from(u16::from_be_bytes([table[list + 2], table[list + 3]]));
    table[lookup..lookup + 2].copy_from_slice(&9_u16.to_be_bytes());
    rejects(table, UiError::PositioningLimit);
}

#[test]
fn repeated_feature_references_are_rejected_before_expansion() {
    let mut table = hostile::repeated_features(16384);
    rejects(table.clone(), UiError::PositioningLimit);
    let list = usize::from(u16::from_be_bytes([table[8], table[9]]));
    let lookup = list + usize::from(u16::from_be_bytes([table[list + 2], table[list + 3]]));
    table[lookup..lookup + 2].copy_from_slice(&9_u16.to_be_bytes());
    rejects(table, UiError::PositioningLimit);
}

#[test]
fn repeated_features_within_budget_are_deduplicated_before_their_lookup_arrays() {
    let bytes = font(Some(hostile::repeated_features(512)), None);
    let mut faces = FaceBook::default();
    let id = faces.load(bytes, 0, 20.).unwrap();
    assert_eq!(
        faces.pair_adjustments(id, &[1, 2]).unwrap()[0]
            .x_advance
            .to_bits(),
        (-2_f32).to_bits()
    );
}

#[test]
fn malformed_unrelated_feature_rejects_instead_of_using_legacy_kern() {
    rejects(hostile::malformed_feature(), UiError::InvalidGpos);
}

#[test]
fn sorted_second_glyph_records_and_size_changes() {
    let lookup = Lookup::new(vec![hostile::multiple_seconds()], false);
    let mut faces = FaceBook::default();
    let id = faces
        .load(font(Some(default_gpos(&[lookup])), None), 0, 10.)
        .unwrap();
    let pairs = faces.pair_adjustments(id, &[0, 1, 2]).unwrap();
    assert_eq!(
        pairs
            .iter()
            .map(|p| (p.first, p.second))
            .collect::<Vec<_>>(),
        vec![(1, 0), (1, 1), (1, 2)]
    );
    for (pair, expected) in pairs.iter().zip([-0.3_f32, -0.5, -1.]) {
        assert!((pair.x_advance - expected).abs() < 0.00001);
    }
    faces.set_size(id, 40.).unwrap();
    let pairs = faces.pair_adjustments(id, &[1, 2]).unwrap();
    assert_eq!(
        pairs
            .iter()
            .find(|p| p.second == 2)
            .unwrap()
            .x_advance
            .to_bits(),
        (-4_f32).to_bits()
    );
}
