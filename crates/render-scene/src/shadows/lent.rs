//! What the shadowed route lends of its cascades to a game's features.
use super::{DirectionalShadowSettings, ShadowMap, cascades::CascadeViews};
use crate::compositor::{FrameShadows, ShadowCascade};

/// The cascades of the frame as the stock receivers will read them, or `None` when the frame has
/// no receivers and so no casters wrote the map. The values are the ones the receiver uniform is
/// encoded from, so a feature that samples with them agrees with the stock receivers.
pub(super) fn lend(
    map: &ShadowMap,
    cascades: &CascadeViews,
    settings: DirectionalShadowSettings,
) -> Option<FrameShadows> {
    if cascades.no_receivers {
        return None;
    }
    let lent = cascades
        .matrices
        .iter()
        .enumerate()
        .map(|(index, &view_projection)| ShadowCascade {
            view_projection,
            split_distance: cascades.splits[index],
            depth_bias: cascades.depth_bias[index],
        })
        .collect();
    Some(FrameShadows {
        depth: map.depth.receiver_view().clone(),
        sampler: map.sampler.clone(),
        resolution: map.depth.resolution(),
        cascades: lent,
        selection: cascades.selection(),
        near: cascades.near,
        view_depth: cascades.view_depth,
        blend: cascades.blend,
        multiplier: cascades.multiplier,
        fade_out: cascades.fade_out,
        normal_bias_meters: settings.normal_bias_meters,
    })
}
