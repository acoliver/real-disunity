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

#[test]
fn unsubmitted_effect_statistics_have_no_installation_or_receivers() {
    use super::{EffectStatistics, PassExecution, PassInstallation, ReceiverBinding};
    let stats = EffectStatistics::default();
    assert_eq!(stats.shadows.installation, PassInstallation::Absent);
    assert_eq!(stats.shadows.receivers.meshes, ReceiverBinding::Unbound);
    assert_eq!(stats.shadows.receivers.terrain, ReceiverBinding::Unbound);
    assert_eq!(stats.ssao.installation, PassInstallation::Absent);
    assert_eq!(stats.ssao.prepass, PassExecution::Skipped);
}
