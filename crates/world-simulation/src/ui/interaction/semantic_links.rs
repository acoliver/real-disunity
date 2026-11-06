use super::targets::Target;
use super::{Targets, UiAccessAction, UiNodeId, UiRole, UiSemanticNode, UiSemanticValue};
use crate::ui::UiLayout;
use crate::{ComponentDesc, World};
use glam::Vec2;
use std::collections::BTreeMap;

pub(super) fn append(
    world: &World,
    layout: &UiLayout,
    targets: &Targets<'_>,
    nodes: &mut BTreeMap<UiNodeId, UiSemanticNode>,
) {
    let runs: BTreeMap<_, _> = world
        .published_components()
        .filter_map(|c| match &c.desc {
            ComponentDesc::Text(run) if run.enabled => Some((c.object, run)),
            _ => None,
        })
        .collect();
    for target in targets.entries.iter().filter(|t| t.eligible) {
        let Some(link) = target.id.link else { continue };
        let run = runs[&target.id.object];
        let span = run
            .spans
            .iter()
            .find(|s| s.link == Some(link))
            .expect("link span");
        let id = UiNodeId::from(target.id);
        let parent = nodes
            .get_mut(&UiNodeId::Object(target.id.object))
            .expect("paragraph node");
        parent.disabled = false;
        parent.children.push(id);
        nodes.insert(
            id,
            UiSemanticNode {
                id,
                role: UiRole::Link,
                label: run.text[span.range.clone()].to_owned(),
                value: UiSemanticValue::None,
                bounds: bounds(target, layout),
                children: Vec::new(),
                actions: vec![UiAccessAction::Focus, UiAccessAction::Press],
                disabled: false,
            },
        );
    }
}

fn bounds(target: &Target<'_>, layout: &UiLayout) -> [f64; 4] {
    let rect = target.rect;
    let mut low = Vec2::INFINITY;
    let mut high = Vec2::NEG_INFINITY;
    for [min, max] in target.regions.as_ref().expect("link glyph regions") {
        for point in [*min, Vec2::new(max.x, min.y), *max, Vec2::new(min.x, max.y)] {
            let screen = rect
                .drawn_point((point - rect.pivot * rect.size).extend(0.))
                .truncate();
            low = low.min(screen);
            high = high.max(screen);
        }
    }
    [
        f64::from(low.x),
        f64::from(layout.viewport.y) - f64::from(high.y),
        f64::from(high.x),
        f64::from(layout.viewport.y) - f64::from(low.y),
    ]
}
