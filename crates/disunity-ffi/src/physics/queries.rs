use super::*;

impl DisunityWorld {
    pub(crate) fn overlap_point(
        &self,
        point: [f32; 2],
        layer_mask: Option<u32>,
        include_triggers: Option<bool>,
    ) -> Result<Value, String> {
        if point.iter().any(|v| !v.is_finite()) {
            return Err("point must be finite".into());
        }
        let p = Shape::Circle {
            center: point.into(),
            radius: 0.0,
        };
        let mut hits = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for c in self.colliders()? {
            if query_allowed(&c, (layer_mask, include_triggers))
                && geometry::overlap(&p, &c.shape).is_some()
                && seen.insert(c.id)
            {
                let mut snapshot = self.components[self.component_index(c.id)?]
                    .as_ref()
                    .unwrap()
                    .snapshot();
                snapshot["attached_body"] = json!(c.body.map(|b| b.1));
                hits.push(snapshot);
            }
        }
        Ok(json!(hits))
    }
    pub(crate) fn raycast(
        &self,
        origin: [f32; 2],
        direction: [f32; 2],
        distance: f32,
        filter: (Option<u32>, Option<bool>),
    ) -> Result<Value, String> {
        if origin
            .iter()
            .chain(direction.iter())
            .any(|v| !v.is_finite())
            || !distance.is_finite()
            || distance < 0.0
        {
            return Err("ray values must be finite; distance nonnegative".into());
        }
        let d = Vec2::from(direction)
            .try_normalize()
            .ok_or("ray direction must be nonzero")?;
        let o = Vec2::from(origin);
        let mut hits = Vec::new();
        for c in self.colliders()? {
            if !query_allowed(&c, filter) {
                continue;
            }
            if let Some((t, n)) = geometry::raycast(&c.shape, o, d, distance) {
                hits.push((t,json!({"object":c.object,"collider":c.id,"attached_body":c.body.map(|b|b.1),"point":(o+d*t).to_array(),"normal":n.to_array(),"distance":t})));
            }
        }
        hits.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut seen = std::collections::BTreeSet::new();
        hits.retain(|(_, hit)| seen.insert(hit["collider"].as_u64().unwrap()));
        Ok(json!(hits.into_iter().map(|(_, v)| v).collect::<Vec<_>>()))
    }
}
fn query_allowed(c: &Collider, filter: (Option<u32>, Option<bool>)) -> bool {
    filter.0.unwrap_or(u32::MAX) & (1 << c.layer) != 0 && (filter.1.unwrap_or(true) || !c.trigger)
}
