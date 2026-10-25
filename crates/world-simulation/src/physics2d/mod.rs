//! Native 2D simulation owned by World. Distances are metres; angles are radians.
//! A step is transactional and replaces the previous step's contact observations.
//! `next_contact` checks live eligibility at delivery, including mutations during dispatch.
mod body;
mod ccd;
mod contacts;
mod geometry;
mod motion;
mod queries;
mod shapes;
mod solver;
mod tiles;

pub use body::{Body2d, BodyType, PhysicsPlane, PhysicsSettings};
pub use contacts::{ContactEvent, ContactPhase};
pub use queries::{QueryFilter, RayHit};
pub use shapes::{Collider2d, Shape2d};
pub use tiles::{TileCell, TileGrid, TileShape};

use crate::{ComponentHandle, ObjectHandle, World, WorldError};
use geometry::Shape;
use glam::Vec2;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PhysicsError {
    #[error("body requires positive finite mass, finite motion and restitution in 0..1")]
    InvalidBody,
    #[error("invalid convex shape or shape budget exceeded")]
    InvalidShape,
    #[error("invalid physics settings")]
    InvalidSettings,
    #[error("delta must be finite and nonnegative")]
    InvalidDelta,
    #[error("invalid finite query or zero ray direction")]
    InvalidQuery,
    #[error("collider transform is not planar, invertible or circle preserving")]
    InvalidTransform,
    #[error("continuous collision iteration budget exceeded")]
    IterationLimit,
    #[error("layer must be in 0..31")]
    InvalidLayer,
    #[error("static collider below a simulated body is unsupported")]
    UnsupportedHierarchy,
    #[error("physics arithmetic exceeded finite range")]
    NumericOverflow,
}

type Pair = (ComponentHandle, ComponentHandle);
type Contacts = BTreeMap<Pair, ContactEvent>;
#[derive(Clone, Default)]
pub(crate) struct Physics {
    settings: PhysicsSettings,
    contacts: Contacts,
    events: Vec<ContactEvent>,
    cursor: usize,
    masks: Option<[u32; 32]>,
}
#[derive(Clone)]
struct Collider {
    id: ComponentHandle,
    object: ObjectHandle,
    owner: ObjectHandle,
    body: Option<ComponentHandle>,
    kind: BodyType,
    shape: Shape,
    trigger: bool,
    layer: u8,
    mask: u32,
    restitution: f32,
}
#[derive(Clone)]
struct Motion {
    component: ComponentHandle,
    start: Vec2,
    position: Vec2,
    angle: f32,
    velocity: Vec2,
    angular: f32,
    inverse_mass: f32,
    inverse_inertia: f32,
}
type Motions = BTreeMap<ObjectHandle, Motion>;
type Impact = (f32, usize, usize, Vec2);

impl World {
    pub fn physics_settings(&self) -> PhysicsSettings {
        self.physics.settings
    }
    pub fn set_physics_settings(&mut self, settings: PhysicsSettings) -> Result<(), WorldError> {
        settings.validate()?;
        self.physics.settings = settings;
        Ok(())
    }
    pub fn set_layer_collision(&mut self, a: u8, b: u8, enabled: bool) -> Result<(), WorldError> {
        if a > 31 || b > 31 {
            return Err(PhysicsError::InvalidLayer.into());
        }
        let masks = self.physics.masks.get_or_insert([u32::MAX; 32]);
        for (a, b) in [(a, b), (b, a)] {
            if enabled {
                masks[a as usize] |= 1 << b;
            } else {
                masks[a as usize] &= !(1 << b);
            }
        }
        Ok(())
    }
    /// Sole fixed-step physics entry point. Scheduling and time accumulation belong to the caller.
    /// Errors do not publish partial motion or contact state.
    pub fn step_physics(&mut self, dt: f32) -> Result<(), WorldError> {
        self.ensure_no_advance()?;
        self.step_physics_inner(dt)
    }
    pub(crate) fn step_physics_inner(&mut self, dt: f32) -> Result<(), WorldError> {
        if !dt.is_finite() || dt < 0.0 {
            return Err(PhysicsError::InvalidDelta.into());
        }
        let base = self.physics_colliders()?;
        let mut motions = self.collect_motions(dt, &base)?;
        self.validate_static_hierarchies(&base, &motions)?;
        let mut remaining = dt;
        let mut touched = Contacts::new();
        let mut budget = self.physics.settings.max_iterations;
        while remaining > 0.0 {
            if budget == 0 {
                return Err(PhysicsError::IterationLimit.into());
            }
            budget -= 1;
            let colliders = motion::moved_colliders(&base, &motions)?;
            let horizon = remaining;
            let earliest = solver::earliest_impact(&colliders, &motions, horizon, &mut budget)?;
            let advance = earliest.map_or(horizon, |e| e.0);
            solver::collect_triggers(&colliders, &motions, advance, &mut touched, &mut budget)?;
            motion::advance(&mut motions, advance)?;
            remaining -= advance;
            if let Some(impact) = earliest {
                let at_impact = motion::moved_colliders(&base, &motions)?;
                solver::resolve(&at_impact, &mut motions, impact, &mut touched);
            }
        }
        let final_colliders = motion::moved_colliders(&base, &motions)?;
        let mut contacts = Contacts::new();
        for (i, a) in final_colliders.iter().enumerate() {
            for b in &final_colliders[i + 1..] {
                if pair_allowed(a, b)
                    && let Some((n, _)) = geometry::overlap(&a.shape, &b.shape)
                {
                    let event =
                        contacts::contact(a, b, n, geometry::contact_point(&a.shape, &b.shape, n));
                    touched.insert((a.id, b.id), event.clone());
                    contacts.insert((a.id, b.id), event);
                }
            }
        }
        self.commit_motions(&motions)?;
        self.finish_contacts(&touched, contacts);
        Ok(())
    }
}
fn pair_allowed(a: &Collider, b: &Collider) -> bool {
    a.id != b.id
        && a.owner != b.owner
        && a.mask & (1 << b.layer) != 0
        && b.mask & (1 << a.layer) != 0
        && (a.kind == BodyType::Dynamic
            || b.kind == BodyType::Dynamic
            || ((a.trigger || b.trigger)
                && (a.kind == BodyType::Kinematic || b.kind == BodyType::Kinematic)))
}
