//! Standalone asset work uses the same tickets, nested scopes and failure teardown as scripts.
use crate::{Advance, LimitKind, ScriptMessage, World, WorldError};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Entered {
    #[default]
    Dormant,
    Awake,
    Enabled,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Lifetime {
    #[default]
    Live,
    Pending,
    Retiring,
}
#[derive(Clone, Copy, Default)]
struct State {
    entered: Entered,
    lifetime: Lifetime,
}
#[derive(Clone, Default)]
pub(crate) struct Assets {
    states: BTreeMap<u64, State>,
}

impl World {
    pub fn create_script_asset(&mut self, identity: u64) -> Result<(), WorldError> {
        self.ensure_allocation_allowed()?;
        if identity == 0
            || self.assets.states.contains_key(&identity)
            || self.scheduler.delayed_tasks == Some(identity)
        {
            return Err(WorldError::InvalidObject);
        }
        if self.assets.states.len() >= self.assets_limit as usize {
            return Err(WorldError::CapacityExceeded(LimitKind::Objects));
        }
        self.assets.states.insert(identity, State::default());
        Ok(())
    }
    pub fn activate_script_asset(&mut self, identity: u64) -> Result<Advance, WorldError> {
        self.ensure_allocation_allowed()?;
        if !self.assets.states.contains_key(&identity) {
            return Err(WorldError::InvalidObject);
        }
        self.open_asset_scope(identity)
    }
    pub fn destroy_script_asset(&mut self, identity: u64) -> Result<(), WorldError> {
        let state = self
            .assets
            .states
            .get_mut(&identity)
            .ok_or(WorldError::InvalidObject)?;
        if state.lifetime == Lifetime::Live {
            state.lifetime = Lifetime::Pending;
        }
        Ok(())
    }
    pub fn script_asset_alive(&self, identity: u64) -> bool {
        self.assets.states.contains_key(&identity)
    }
    pub(crate) fn fail_script_assets(&mut self) {
        for state in self.assets.states.values_mut() {
            if state.lifetime == Lifetime::Live {
                state.lifetime = Lifetime::Pending;
            }
        }
    }
    pub(crate) fn asset_cleanup(&mut self) -> Vec<(u64, ScriptMessage)> {
        let mut work = Vec::new();
        let mut dormant = Vec::new();
        for (&id, state) in &mut self.assets.states {
            if state.lifetime != Lifetime::Pending {
                continue;
            }
            state.lifetime = Lifetime::Retiring;
            if state.entered == Entered::Enabled {
                work.push((id, ScriptMessage::OnDisable));
            }
            if state.entered == Entered::Dormant {
                dormant.push(id);
            } else {
                work.push((id, ScriptMessage::OnDestroy));
            }
        }
        for id in dormant {
            self.assets.states.remove(&id);
        }
        work
    }
    pub(crate) fn enter_asset(&mut self, identity: u64, message: ScriptMessage) -> bool {
        let Some(state) = self.assets.states.get_mut(&identity) else {
            return false;
        };
        match message {
            ScriptMessage::Awake
                if state.entered == Entered::Dormant && state.lifetime == Lifetime::Live =>
            {
                state.entered = Entered::Awake;
            }
            ScriptMessage::OnEnable
                if state.entered == Entered::Awake && state.lifetime == Lifetime::Live =>
            {
                state.entered = Entered::Enabled;
            }
            ScriptMessage::OnDisable if state.entered == Entered::Enabled => {
                state.entered = Entered::Awake;
            }
            ScriptMessage::OnDestroy => {
                self.assets.states.remove(&identity);
            }
            _ => return false,
        }
        true
    }
}
