use crate::{
    DisunityWorld,
    components::{BodyType, Fields},
    geometry::{self, Shape},
};
use glam::{Vec2, Vec3};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

mod motion;
mod queries;
mod shapes;
mod solver;

#[derive(Clone, Serialize)]
pub(crate) struct Contact {
    pub a: u64,
    pub b: u64,
    pub body_a: Option<u64>,
    pub body_b: Option<u64>,
    pub rigidbody_a: Option<u64>,
    pub rigidbody_b: Option<u64>,
    pub collider_a: u64,
    pub collider_b: u64,
    pub trigger: bool,
    pub normal: [f32; 2],
    pub point: [f32; 2],
}
#[derive(Clone)]
pub(crate) struct Physics {
    pub gravity: [f32; 2],
    pub contacts: Contacts,
    pub events: Vec<Value>,
    pub layer_masks: [u32; 32],
}
impl Default for Physics {
    fn default() -> Self {
        Self {
            gravity: [0.0, -9.81],
            contacts: BTreeMap::new(),
            events: Vec::new(),
            layer_masks: [u32::MAX; 32],
        }
    }
}
impl Physics {
    pub(crate) fn set_layers(&mut self, a: u8, b: u8, ignore: bool) -> Result<Value, String> {
        if a > 31 || b > 31 {
            return Err("layers must be in 0..31".into());
        }
        for (a, b) in [(a, b), (b, a)] {
            if ignore {
                self.layer_masks[a as usize] &= !(1 << b);
            } else {
                self.layer_masks[a as usize] |= 1 << b;
            }
        }
        Ok(Value::Null)
    }
}
struct Collider {
    id: u64,
    object: u64,
    owner: u64,
    body: Option<(u64, u64)>,
    body_type: BodyType,
    shape: Shape,
    trigger: bool,
    layer: u8,
    mask: u32,
    restitution: f32,
    friction: f32,
}
#[derive(Clone)]
struct Motion {
    object: u64,
    component: usize,
    velocity: Vec2,
    angular: f32,
    inverse_mass: Vec2,
    inverse_inertia: f32,
    sleeping: bool,
}
type Motions = BTreeMap<u64, Motion>;
type Contacts = BTreeMap<(u64, u64), Contact>;
type Impact = (f32, usize, usize, Vec2);

impl DisunityWorld {
    pub(crate) fn physics_step(&mut self, dt: f32) -> Result<Value, String> {
        let fixed_frame = self
            .clock
            .fixed_frame
            .checked_add(1)
            .ok_or("fixed frame overflow")?;
        crate::input::validate_dt(dt)?;
        if self.physics.events.len() > 65536 {
            return Err("contact queue budget exceeded; drain events.read".into());
        }
        let mut motions = self.collect_motions(dt)?;
        let mut remaining = dt;
        let mut touched = BTreeMap::new();
        let mut iterations = 0;
        while remaining > 1e-7 {
            iterations += 1;
            if iterations > 2048 {
                return Err("continuous collision iteration budget exceeded".into());
            }
            let colliders = self.colliders()?;
            let horizon = motion::angular_horizon(&motions, remaining);
            let earliest = solver::earliest_impact(&colliders, &motions, horizon);
            let advance = earliest.map_or(horizon, |e| e.0);
            solver::collect_triggers(&colliders, &motions, advance, &mut touched);
            self.advance(&mut motions, advance)?;
            remaining -= advance;
            if let Some(impact) = earliest {
                self.resolve_impact(&colliders, &mut motions, impact, &mut touched)?;
            }
        }
        self.finish_motion(&motions, dt);
        self.finish_contacts(&mut touched)?;
        self.clock.fixed_frame = fixed_frame;
        if self.strict.is_none()
            && let Ok(endpoint) = self.clock.endpoint()
        {
            self.input.advance(
                crate::input::projection::Phase::Fixed,
                endpoint,
                fixed_frame,
            )?;
        }
        Ok(json!(self.clock))
    }
    fn finish_contacts(&mut self, touched: &mut Contacts) -> Result<(), String> {
        let colliders = self.colliders()?;
        let mut contacts = BTreeMap::new();
        for (i, a) in colliders.iter().enumerate() {
            for b in &colliders[i + 1..] {
                if !pair_allowed(a, b) {
                    continue;
                }
                if let Some((n, _)) = geometry::overlap(&a.shape, &b.shape) {
                    let c = contact(a, b, n, geometry::contact_point(&a.shape, &b.shape, n));
                    touched.insert((a.id, b.id), c.clone());
                    contacts.insert((a.id, b.id), c);
                }
            }
        }
        for (key, c) in touched.iter() {
            self.physics.events.push(event(
                c,
                if self.physics.contacts.contains_key(key) {
                    "stay"
                } else {
                    "enter"
                },
            ));
        }
        for (key, c) in self.physics.contacts.iter().chain(
            touched
                .iter()
                .filter(|(k, _)| !self.physics.contacts.contains_key(k)),
        ) {
            if !contacts.contains_key(key) {
                self.physics.events.push(event(c, "exit"));
            }
        }
        self.physics.contacts = contacts;
        Ok(())
    }
}
fn pair_allowed(a: &Collider, b: &Collider) -> bool {
    a.owner != b.owner
        && a.mask & (1 << b.layer) != 0
        && b.mask & (1 << a.layer) != 0
        && bodies_participate(a, b)
}
fn bodies_participate(a: &Collider, b: &Collider) -> bool {
    a.body_type == BodyType::Dynamic
        || b.body_type == BodyType::Dynamic
        || ((a.trigger || b.trigger)
            && (a.body_type == BodyType::Kinematic || b.body_type == BodyType::Kinematic))
}
fn contact(a: &Collider, b: &Collider, normal: Vec2, point: Vec2) -> Contact {
    Contact {
        a: a.object,
        b: b.object,
        body_a: a.body.map(|b| b.0),
        body_b: b.body.map(|b| b.0),
        rigidbody_a: a.body.map(|b| b.1),
        rigidbody_b: b.body.map(|b| b.1),
        collider_a: a.id,
        collider_b: b.id,
        trigger: a.trigger || b.trigger,
        normal: normal.to_array(),
        point: point.to_array(),
    }
}
fn event(c: &Contact, phase: &str) -> Value {
    let mut v = json!(c);
    v["phase"] = json!(phase);
    v
}
