use super::{Collider, Contacts, Impact, Motions, contacts::contact, geometry, pair_allowed};
use crate::{ObjectHandle, WorldError};
use glam::Vec2;

fn correct_angular_overlap(
    motions: &mut Motions,
    a: &Collider,
    b: &Collider,
    normal: Vec2,
    depth: f32,
) {
    let point = geometry::contact_point(&a.shape, &b.shape, normal);
    let arm = |owner| {
        motions
            .get(&owner)
            .map_or(Vec2::ZERO, |m| point - m.position)
    };
    let (ra, rb) = (arm(a.owner), arm(b.owner));
    let denominator =
        effective_mass(motions, a.owner, ra, normal) + effective_mass(motions, b.owner, rb, normal);
    if denominator <= 0.0 {
        return;
    }
    let impulse = (depth + 4e-5) / denominator;
    for (owner, arm, sign) in [(a.owner, ra, -1.0), (b.owner, rb, 1.0)] {
        if let Some(m) = motions.get_mut(&owner) {
            m.angle += sign * arm.perp_dot(normal) * impulse * m.inverse_inertia;
        }
    }
}

fn contact_response(
    a: &Collider,
    b: &Collider,
    motions: &Motions,
    time: f32,
    normal: Vec2,
) -> Result<f32, WorldError> {
    let sa = super::ccd::pose(a, motions, time)?;
    let sb = super::ccd::pose(b, motions, time)?;
    let point = geometry::contact_point(&sa, &sb, normal);
    let mass = |owner| {
        let arm = motions
            .get(&owner)
            .map_or(Vec2::ZERO, |m| point - (m.position + m.velocity * time));
        effective_mass(motions, owner, arm, normal)
    };
    Ok(mass(a.owner) + mass(b.owner))
}

pub(super) fn resolve(
    colliders: &[Collider],
    motions: &mut Motions,
    impact: Impact,
    touched: &mut Contacts,
    bounce_threshold: f32,
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
    let material = collider_a.material.combine(collider_b.material);
    let closing_speed = relative.dot(normal).max(0.0);
    let restitution = if (collider_a.material_backed || collider_b.material_backed)
        && closing_speed <= bounce_threshold
    {
        0.0
    } else {
        material.bounciness()
    };
    let impulse = (1.0 + restitution) * closing_speed / denominator;
    apply_impulse(motions, collider_a.owner, ra, -normal * impulse);
    apply_impulse(motions, collider_b.owner, rb, normal * impulse);
    friction_impulse(
        motions,
        (collider_a.owner, collider_b.owner),
        (ra, rb),
        normal,
        material.friction() * impulse,
    );
    correct_positions(colliders, motions, impact);
}
fn correct_positions(colliders: &[Collider], motions: &mut Motions, impact: Impact) {
    let (_, a, b, normal) = impact;
    let (a, b) = (&colliders[a], &colliders[b]);
    let depth = geometry::overlap(&a.shape, &b.shape).map_or(0.0, |(_, d)| d);
    let ia = inverse_mass(motions, a.owner);
    let ib = inverse_mass(motions, b.owner);
    let denominator = normal.dot((ia + ib) * normal);
    if denominator == 0.0 {
        correct_angular_overlap(motions, a, b, normal, depth);
        return;
    }
    let angular = [a.owner, b.owner]
        .iter()
        .any(|o| motions.get(o).is_some_and(|m| m.angular != 0.0));
    let clearance = if angular { 4e-5 } else { 1e-6 };
    let correction = normal * ((depth + clearance) / denominator);
    if let Some(m) = motions.get_mut(&a.owner) {
        m.position -= correction * ia;
    }
    if let Some(m) = motions.get_mut(&b.owner) {
        m.position += correction * ib;
    }
}
fn friction_impulse(
    m: &mut Motions,
    owners: (ObjectHandle, ObjectHandle),
    arms: (Vec2, Vec2),
    normal: Vec2,
    limit: f32,
) {
    if limit == 0.0 {
        return;
    }
    let (a, b) = owners;
    let (ra, rb) = arms;
    let tangent = normal.perp();
    let denominator = effective_mass(m, a, ra, tangent) + effective_mass(m, b, rb, tangent);
    if denominator <= 0.0 {
        return;
    }
    let speed = (point_velocity(m, a, ra) - point_velocity(m, b, rb)).dot(tangent);
    let impulse = (speed / denominator).clamp(-limit, limit);
    apply_impulse(m, a, ra, -tangent * impulse);
    apply_impulse(m, b, rb, tangent * impulse);
}
fn inverse_mass(m: &Motions, o: ObjectHandle) -> Vec2 {
    m.get(&o).map_or(Vec2::ZERO, |m| m.inverse_mass)
}
fn point_velocity(m: &Motions, o: ObjectHandle, r: Vec2) -> Vec2 {
    m.get(&o)
        .map_or(Vec2::ZERO, |m| m.velocity + r.perp() * m.angular)
}
fn effective_mass(m: &Motions, o: ObjectHandle, r: Vec2, n: Vec2) -> f32 {
    m.get(&o).map_or(0.0, |m| {
        n.dot(m.inverse_mass * n) + m.inverse_inertia * r.perp_dot(n).powi(2)
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
                || (!responsive(motions, a.owner) && !responsive(motions, b.owner))
            {
                continue;
            }
            if let Some((t, n)) = super::ccd::sweep(a, b, motions, remaining, budget)?
                && earliest.is_none_or(|e| t < e.0)
                && contact_response(a, b, motions, t, n)? > 0.0
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
            {
                continue;
            }
            if let Some((t, n)) = super::ccd::sweep(a, b, motions, advance, budget)? {
                if !a.trigger && !b.trigger && contact_response(a, b, motions, t, n)? > 0.0 {
                    continue;
                }
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

fn responsive(motions: &Motions, owner: ObjectHandle) -> bool {
    motions
        .get(&owner)
        .is_some_and(|m| m.inverse_mass != Vec2::ZERO || m.inverse_inertia != 0.0)
}
