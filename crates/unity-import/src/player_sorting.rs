//! Sorting groups of a player scene as plan components.

use worldrunner_unity_serialized::sorting_group::SortingGroup;

use crate::{PlayerError, plan::PlanComponent};

/// The plan sorting group for a decoded one. A group whose stored form does not decode plans the
/// engine's defaults (enabled, layer 0, order 0, sorting with its parent) and is listed once in
/// `differences`.
///
/// # Errors
/// Fails when the object could not be read from the file, which is not a layout mismatch.
pub(crate) fn plan_sorting_group(
    owner: &str,
    group: Result<SortingGroup, PlayerError>,
    differences: &mut Vec<String>,
) -> Result<PlanComponent, PlayerError> {
    match group {
        Ok(group) => Ok(PlanComponent::SortingGroup {
            enabled: group.enabled,
            sorting_layer_id: group.sorting_layer_id,
            sorting_order: group.sorting_order.into(),
            sort_at_root: group.sort_at_root,
        }),
        Err(PlayerError::Serialized { source, .. }) => {
            differences.push(format!(
                "{owner}: SortingGroup does not decode ({source}), so its defaults are planned"
            ));
            Ok(PlanComponent::SortingGroup {
                enabled: true,
                sorting_layer_id: 0,
                sorting_order: 0,
                sort_at_root: false,
            })
        }
        Err(other) => Err(other),
    }
}
