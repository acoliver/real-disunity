use super::{Collider, Contacts, Impact, Motions, contacts::contact, geometry, pair_allowed};
use crate::{ObjectHandle, WorldError};
use glam::Vec2;

pub(super) fn resolve(
    colliders: &[Collider],
    motions: &mut Motions,
    impact: Impact,
    touched: &mut Contacts,
) {
    let (_, index_a, index_b, normal) = impact;
    let (collider_a, collider_b) = (&colliders[index_a], &colliders[index_b]);
    let sa = &collider_a.shape;
    let sb = &collider_b.shape;
    let point = geometry::contact_point(sa, sb, normal);
    touched.insert(
        (collider_a.id, collider_b.id),
        contact(collider_a, collider_b, normal, point),
    );
    let ra = motions
        .get(&collider_a.owner)
        .map_or(Vec2::ZERO, |m| point - m.position);
    let rb = motions
        .get(&collider_b.owner)
        .map_or(Vec2::ZERO, |m| point - m.position);
    let denominator = effective_mass(motions, collider_a.owner, ra, normal)
        + effective_mass(motions, collider_b.owner, rb, normal);
    if denominator <= 0.0 {
        return;
    }
    let relative = point_velocity(motions, collider_a.owner, ra)
        - point_velocity(motions, collider_b.owner, rb);
    let impulse = (1.0 + collider_a.restitution.max(collider_b.restitution))
        * relative.dot(normal).max(0.0)
        / denominator;
    apply_impulse(motions, collider_a.owner, ra, -normal * impulse);
    apply_impulse(motions, collider_b.owner, rb, normal * impulse);
    let depth = geometry::overlap(sa, sb).map_or(0.0, |(_, d)| d);
    let ia = inverse_mass(motions, collider_a.owner);
    let ib = inverse_mass(motions, collider_b.owner);
    let angular = [collider_a.owner, collider_b.owner]
        .iter()
        .any(|o| motions.get(o).is_some_and(|m| m.angular != 0.0));
    let clearance = if angular { 4e-5 } else { 1e-6 };
    let correction = normal * ((depth + clearance) / (ia + ib));
    if let Some(m) = motions.get_mut(&collider_a.owner) {
        m.position -= correction * ia;
    }
    if let Some(m) = motions.get_mut(&collider_b.owner) {
        m.position += correction * ib;
    }
}
fn inverse_mass(m: &Motions, o: ObjectHandle) -> f32 {
    m.get(&o).map_or(0.0, |m| m.inverse_mass)
}
fn point_velocity(m: &Motions, o: ObjectHandle, r: Vec2) -> Vec2 {
    m.get(&o)
        .map_or(Vec2::ZERO, |m| m.velocity + r.perp() * m.angular)
}
fn effective_mass(m: &Motions, o: ObjectHandle, r: Vec2, n: Vec2) -> f32 {
    m.get(&o).map_or(0.0, |m| {
        m.inverse_mass + m.inverse_inertia * r.perp_dot(n).powi(2)
    })
}
fn apply_impulse(m: &mut Motions, o: ObjectHandle, r: Vec2, impulse: Vec2) {
    if let Some(m) = m.get_mut(&o) {
        if impulse != Vec2::ZERO {
            m.sleeping = false;
        }
        m.velocity += impulse * m.inverse_mass;
        m.angular += r.perp_dot(impulse) * m.inverse_inertia;
    }
}
pub(super) fn earliest_impact(
    colliders: &[Collider],
    motions: &Motions,
    remaining: f32,
    budget: &mut u32,
) -> Result<Option<Impact>, WorldError> {
    let mut earliest: Option<Impact> = None;
    for (i, a) in colliders.iter().enumerate() {
        for (j, b) in colliders.iter().enumerate().skip(i + 1) {
            if (motions.get(&a.owner).is_none_or(|m| m.sleeping)
                && motions.get(&b.owner).is_none_or(|m| m.sleeping))
                || !pair_allowed(a, b)
                || a.trigger
                || b.trigger
                || inverse_mass(motions, a.owner) + inverse_mass(motions, b.owner) == 0.0
            {
                continue;
            }
            if let Some((t, n)) = super::ccd::sweep(a, b, motions, remaining, budget)?
                && earliest.is_none_or(|e| t < e.0)
            {
                earliest = Some((t, i, j, n));
            }
        }
    }
    Ok(earliest)
}
pub(super) fn collect_triggers(
    colliders: &[Collider],
    motions: &Motions,
    advance: f32,
    touched: &mut Contacts,
    budget: &mut u32,
) -> Result<(), WorldError> {
    for (i, a) in colliders.iter().enumerate() {
        for b in &colliders[i + 1..] {
            if (motions.get(&a.owner).is_none_or(|m| m.sleeping)
                && motions.get(&b.owner).is_none_or(|m| m.sleeping))
                || !pair_allowed(a, b)
                || (!a.trigger && !b.trigger)
            {
                continue;
            }
            if let Some((t, n)) = super::ccd::sweep(a, b, motions, advance, budget)? {
                let sa = super::ccd::pose(a, motions, t)?;
                let sb = super::ccd::pose(b, motions, t)?;
                touched.insert(
                    (a.id, b.id),
                    contact(a, b, n, geometry::contact_point(&sa, &sb, n)),
                );
            }
        }
    }
    Ok(())
}
