use super::statistics::TerrainPageStatistics;

#[test]
fn page_counts_distinguish_exact_residency_from_pending_selection() {
    let counts = TerrainPageStatistics::from_residency([true, false, true]);
    assert_eq!(counts.requested, 3);
    assert_eq!(counts.resident, 2);
    assert_eq!(counts.pending, 1);
    assert_eq!(
        TerrainPageStatistics::from_residency([]),
        TerrainPageStatistics::default()
    );
}
