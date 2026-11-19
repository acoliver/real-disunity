//! The stand-in census in the run report: the summary of a run and the report of one frame.

use super::*;

fn counted(entries: &[(&str, usize)]) -> BTreeMap<String, usize> {
    entries
        .iter()
        .map(|(name, count)| ((*name).to_owned(), *count))
        .collect()
}

fn with_stand_ins(draws: usize, by_property: &[(&str, usize)]) -> Report {
    Report {
        stand_ins: StandInCounts {
            draws,
            by_property: counted(by_property),
        },
        ..Report::default()
    }
}

#[test]
fn the_summary_says_how_many_draws_read_stand_ins_and_which_properties_for_which_reason() {
    let mut report = GpuWorkReport::default();
    report.add(
        &with_stand_ins(3, &[("_A unset", 3), ("_B undeclared", 1)]),
        "frame",
    );
    report.add(
        &with_stand_ins(2, &[("_A unset", 1), ("_C exists-not-loaded", 2)]),
        "flush",
    );
    assert_eq!(
        report.stand_in_lines(),
        [
            "draws with stand-ins 5",
            "stand-ins by property: _A unset 4, _C exists-not-loaded 2, _B undeclared 1",
        ]
    );
}

#[test]
fn a_run_with_no_stand_ins_says_zero_draws_and_names_no_property() {
    let mut report = GpuWorkReport::default();
    report.add(&Report::default(), "frame");
    assert_eq!(report.stand_in_lines(), ["draws with stand-ins 0"]);
}

#[test]
fn the_report_of_a_frame_counts_the_stand_ins_of_that_frame_and_the_summary_keeps_the_totals() {
    let mut report = GpuWorkReport::default();
    report.add(&with_stand_ins(3, &[("_A unset", 3)]), "frame");
    let before = report.stand_ins.clone();
    report.add(&with_stand_ins(1, &[("_B undeclared", 1)]), "flush");
    report.add(&with_stand_ins(2, &[("_A unset", 2)]), "frame");
    assert_eq!(
        report.frame_stand_in_lines(&before),
        [
            "draws with stand-ins 3",
            "stand-ins by property: _A unset 2, _B undeclared 1",
        ]
    );
    assert_eq!(
        report.stand_in_lines(),
        [
            "draws with stand-ins 6",
            "stand-ins by property: _A unset 5, _B undeclared 1",
        ]
    );
    assert_eq!(
        report.frame_stand_in_lines(&report.stand_ins),
        ["draws with stand-ins 0"],
        "a frame that drew with none"
    );
}
