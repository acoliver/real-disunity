//! The retained renderer census in the run report.
use super::*;
use crate::scene_census::{FrameCensus, LeftOut, WhiteReason};

fn census() -> SubstitutionCensus {
    let mut frame = FrameCensus::default();
    frame.drawn(Some(WhiteReason::TextureZero));
    frame.drawn(Some(WhiteReason::TextureZero));
    frame.drawn(None);
    frame.left_out(LeftOut::MeshNotArrived);
    let mut census = SubstitutionCensus::default();
    census.end_frame(frame);
    census
}

#[test]
fn the_summary_names_the_retained_renderers_drawn_white_and_left_out_beside_the_stand_ins() {
    let report = GpuWorkReport {
        retained: Some(census()),
        ..GpuWorkReport::default()
    };
    assert_eq!(
        report
            .stand_in_lines()
            .into_iter()
            .chain(report.retained_lines())
            .collect::<Vec<_>>(),
        [
            "draws with stand-ins 0",
            "retained renderers with texture 0 (last frame) 2 of 3",
            "retained renderers with texture 0 by reason (last frame): texture-zero 2",
            "retained renderers with texture 0 by reason (most in one frame): texture-zero 2",
            "retained objects left out: mesh-not-arrived 1",
        ]
    );
}

#[test]
fn a_run_without_a_retained_scene_prints_no_retained_lines() {
    assert!(GpuWorkReport::default().retained_lines().is_empty());
}

#[test]
fn the_player_gpu_keeps_the_census_it_is_given_for_its_report() {
    let directory = tempfile::tempdir().unwrap();
    let gpu = PlayerGpu::headless(directory.path(), &GpuLaunch::default()).unwrap();
    gpu.note_retained(census());
    assert_eq!(gpu.report().retained.as_ref(), Some(&census()));
    assert_eq!(gpu.report().retained_lines().len(), 4);
}
