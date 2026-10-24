use super::*;

impl DisunityWorld {
    pub(super) fn resolve_impact(
        &mut self,
        colliders: &[Collider],
        motions: &mut Motions,
        impact: Impact,
        touched: &mut Contacts,
    ) -> Result<(), String> {
        let (dt, i, j, n) = impact;
        let a = &colliders[i];
        let b = &colliders[j];
        let sa = a.shape.translated(velocity(motions, a) * dt);
        let sb = b.shape.translated(velocity(motions, b) * dt);
        let point = geometry::contact_point(&sa, &sb, n);
        touched.insert((a.id, b.id), contact(a, b, n, point));
        let ra = point - self.world_trs(a.owner)?.0.truncate();
        let rb = point - self.world_trs(b.owner)?.0.truncate();
        let denominator =
            effective_mass(motions, a.owner, ra, n) + effective_mass(motions, b.owner, rb, n);
        if denominator <= 0.0 {
            return Ok(());
        }
        let relative = point_velocity(motions, a.owner, ra) - point_velocity(motions, b.owner, rb);
        let impulse =
            (1.0 + a.restitution.max(b.restitution)) * relative.dot(n).max(0.0) / denominator;
        apply_impulse(motions, a.owner, ra, -n * impulse);
        apply_impulse(motions, b.owner, rb, n * impulse);
        let tangent = n.perp();
        let mass = effective_mass(motions, a.owner, ra, tangent)
            + effective_mass(motions, b.owner, rb, tangent);
        if mass > 0.0 {
            let speed = (point_velocity(motions, a.owner, ra)
                - point_velocity(motions, b.owner, rb))
            .dot(tangent);
            let limit = (a.friction * b.friction).sqrt() * impulse;
            let friction = (speed / mass).clamp(-limit, limit);
            apply_impulse(motions, a.owner, ra, -tangent * friction);
            apply_impulse(motions, b.owner, rb, tangent * friction);
        }
        let depth = geometry::overlap(&sa, &sb).map_or(0.0, |(_, d)| d);
        self.correct_positions(motions, (a.owner, b.owner), n, depth)
    }
    fn correct_positions(
        &mut self,
        motions: &Motions,
        owners: (u64, u64),
        normal: Vec2,
        depth: f32,
    ) -> Result<(), String> {
        let ia = motions
            .get(&owners.0)
            .map_or(Vec2::ZERO, |m| m.inverse_mass);
        let ib = motions
            .get(&owners.1)
            .map_or(Vec2::ZERO, |m| m.inverse_mass);
        let denominator = ((ia + ib) * normal).dot(normal);
        if denominator <= 0.0 {
            return Ok(());
        }
        let correction = normal * ((depth + 1e-4) / denominator);
        for (h, delta) in [(owners.0, -correction * ia), (owners.1, correction * ib)] {
            if delta != Vec2::ZERO {
                let p = self.world_trs(h)?.0 + delta.extend(0.0);
                self.set_trs(h, Some(p.to_array()), None, None, "world")?;
            }
        }
        Ok(())
    }
}
fn point_velocity(motions: &Motions, owner: u64, r: Vec2) -> Vec2 {
    motions
        .get(&owner)
        .map_or(Vec2::ZERO, |m| m.velocity + r.perp() * m.angular)
}
fn effective_mass(motions: &Motions, owner: u64, r: Vec2, n: Vec2) -> f32 {
    motions.get(&owner).map_or(0.0, |m| {
        (m.inverse_mass * n).dot(n) + m.inverse_inertia * r.perp_dot(n).powi(2)
    })
}
fn apply_impulse(motions: &mut Motions, owner: u64, r: Vec2, impulse: Vec2) {
    if let Some(m) = motions.get_mut(&owner) {
        let dv = impulse * m.inverse_mass;
        let dw = r.perp_dot(impulse) * m.inverse_inertia;
        m.velocity += dv;
        m.angular += dw;
        if dv.length_squared() > 1e-12 || dw.abs() > 1e-6 {
            m.sleeping = false;
        }
    }
}
fn velocity(motions: &Motions, c: &Collider) -> Vec2 {
    motions.get(&c.owner).map_or(Vec2::ZERO, |m| m.velocity)
}
pub(super) fn earliest_impact(
    colliders: &[Collider],
    motions: &Motions,
    remaining: f32,
) -> Option<Impact> {
    let mut earliest: Option<Impact> = None;
    for (i, a) in colliders.iter().enumerate() {
        for (j, b) in colliders.iter().enumerate().skip(i + 1) {
            if !pair_allowed(a, b) || a.trigger || b.trigger {
                continue;
            }
            let relative = velocity(motions, a) - velocity(motions, b);
            if let Some((t, n)) = geometry::sweep(&a.shape, &b.shape, relative, remaining) {
                let mass = motions.get(&a.owner).map_or(Vec2::ZERO, |m| m.inverse_mass)
                    + motions.get(&b.owner).map_or(Vec2::ZERO, |m| m.inverse_mass);
                if (mass * n).dot(n) > 0.0 && earliest.is_none_or(|e| t < e.0) {
                    earliest = Some((t, i, j, n));
                }
            }
        }
    }
    earliest
}
pub(super) fn collect_triggers(
    colliders: &[Collider],
    motions: &Motions,
    advance: f32,
    touched: &mut Contacts,
) {
    for (i, a) in colliders.iter().enumerate() {
        for b in &colliders[i + 1..] {
            if !pair_allowed(a, b) || (!a.trigger && !b.trigger) {
                continue;
            }
            if let Some((t, n)) = geometry::sweep(
                &a.shape,
                &b.shape,
                velocity(motions, a) - velocity(motions, b),
                advance,
            ) {
                let sa = a.shape.translated(velocity(motions, a) * t);
                let sb = b.shape.translated(velocity(motions, b) * t);
                touched.insert(
                    (a.id, b.id),
                    contact(a, b, n, geometry::contact_point(&sa, &sb, n)),
                );
            }
        }
    }
}
