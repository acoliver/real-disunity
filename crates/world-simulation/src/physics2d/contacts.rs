use super::{Collider, Contacts};
use crate::{ComponentDesc, ComponentHandle, ObjectHandle, World};
use glam::Vec2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ContactPhase {
    Enter,
    Stay,
    Exit,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ContactEvent {
    pub phase: ContactPhase,
    pub collider_a: ComponentHandle,
    pub collider_b: ComponentHandle,
    pub object_a: ObjectHandle,
    pub object_b: ObjectHandle,
    pub body_a: Option<ComponentHandle>,
    pub body_b: Option<ComponentHandle>,
    pub trigger: bool,
    /// Points from A towards B in the selected plane.
    pub normal: Vec2,
    pub point: Vec2,
}
pub(super) fn contact(a: &Collider, b: &Collider, normal: Vec2, point: Vec2) -> ContactEvent {
    ContactEvent {
        phase: ContactPhase::Enter,
        collider_a: a.id,
        collider_b: b.id,
        object_a: a.object,
        object_b: b.object,
        body_a: a.body,
        body_b: b.body,
        trigger: a.trigger || b.trigger,
        normal,
        point,
    }
}
impl World {
    /// Completed step observations. Exit records deliberately retain retired identities.
    pub fn contacts(&self) -> &[ContactEvent] {
        &self.physics.events
    }
    /// Deliver pending records in pair/phase order. Mutations suppress ordinary contact
    /// callbacks immediately. Historical exit observations remain available in `contacts`.
    pub fn next_contact(&mut self) -> Option<ContactEvent> {
        while let Some(c) = self.physics.events.get(self.physics.cursor).cloned() {
            self.physics.cursor += 1;
            if self.contact_eligible(&c) {
                return Some(c);
            }
        }
        None
    }
    fn contact_eligible(&self, c: &ContactEvent) -> bool {
        [c.collider_a, c.collider_b].iter().all(|h| {
            self.component(*h).is_ok_and(|c| {
                matches!(&c.desc, ComponentDesc::Collider2d(d) if d.enabled)
                    && self.active_in_hierarchy(c.object).unwrap_or(false)
            })
        }) && [c.body_a, c.body_b].iter().flatten().all(|h| {
            self.component(*h)
                .is_ok_and(|c| matches!(&c.desc, ComponentDesc::Body2d(d) if d.enabled))
        })
    }
    pub(super) fn finish_contacts(&mut self, touched: &Contacts, current: Contacts) {
        let mut events = Vec::new();
        for (key, c) in touched {
            let mut event = c.clone();
            event.phase = if self.physics.contacts.contains_key(key) {
                ContactPhase::Stay
            } else {
                ContactPhase::Enter
            };
            events.push(event);
        }
        for (key, c) in self.physics.contacts.iter().chain(
            touched
                .iter()
                .filter(|(k, _)| !self.physics.contacts.contains_key(k)),
        ) {
            if !current.contains_key(key) {
                let mut event = c.clone();
                event.phase = ContactPhase::Exit;
                events.push(event);
            }
        }
        events.sort_by_key(|c| (c.collider_a, c.collider_b, c.phase));
        self.physics.events = events;
        self.physics.cursor = 0;
        self.physics.contacts = current;
    }
}
