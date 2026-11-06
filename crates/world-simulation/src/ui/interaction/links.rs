use super::targets::Target;
use super::{TargetId, Targets, UiRouter};
use crate::ui::{UiError, UiLayout, contribution::clip, text_geometry};
use crate::{ComponentDesc, World, WorldError};
use glam::Vec2;

pub(super) fn targets<'a>(
    world: &World,
    layout: &UiLayout,
    router: &UiRouter,
    target: &Target<'a>,
) -> Result<Vec<Target<'a>>, WorldError> {
    let rect = target.rect;
    let run = world
        .published_components()
        .find_map(|c| match &c.desc {
            ComponentDesc::Text(run) if c.object == rect.object => Some(run),
            _ => None,
        })
        .ok_or(UiError::MissingLayout)?;
    if !run.enabled || !run.spans.iter().any(|s| s.link.is_some()) {
        return Ok(Vec::new());
    }
    let fonts = router
        .fonts
        .as_ref()
        .ok_or(UiError::MissingFont(run.font))?;
    let text = fonts.layout(run, rect.size.max(Vec2::ZERO))?;
    let shift = text_geometry::snap_offset(&text, rect, text_geometry::canvas(world, rect.canvas));
    let roots = crate::ui::graphics::root_canvases(world)?;
    let clip = clip::text_clip(clip::screen_clip(world, layout, rect, &roots), rect);
    let mut result = Vec::new();
    for link in run.spans.iter().filter_map(|s| s.link) {
        let regions: Vec<_> = text
            .glyphs
            .iter()
            .filter(|g| g.link == Some(link))
            .filter_map(|g| text_geometry::clipped(rect, [g.rect_min + shift, g.rect_max + shift]))
            .collect();
        if regions.is_empty() {
            continue;
        }
        result.push(Target {
            id: TargetId {
                object: rect.object,
                link: Some(link),
            },
            rect,
            regions: Some(regions),
            clip: Some(clip),
            eligible: target.eligible,
            blocking: target.blocking,
        });
    }
    Ok(result)
}

impl UiRouter {
    pub(super) fn reveal_target(
        &mut self,
        world: &World,
        layout: &UiLayout,
        id: TargetId,
        targets: &Targets<'_>,
    ) -> Result<(), WorldError> {
        let regions = targets
            .entries
            .iter()
            .find(|t| t.id == id)
            .and_then(|t| t.regions.as_ref());
        let bounds = regions.map(|regions| {
            regions.iter().fold(
                [Vec2::INFINITY, Vec2::NEG_INFINITY],
                |[low, high], [min, max]| [low.min(*min), high.max(*max)],
            )
        });
        self.layout.reveal(world, layout, id.object, bounds)
    }
}
