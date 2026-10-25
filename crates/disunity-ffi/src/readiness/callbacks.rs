use super::{Callback, LIMIT, Phase, eligibility, live_script, spend};
use crate::DisunityWorld;
use serde_json::Value;
#[derive(Clone)]
pub(super) struct Script {
    mask: u16,
    flags: u8,
    last_phase: u64,
    immediate_parent: Option<u64>,
}
const AWAKE: u8 = 1;
const ENABLED: u8 = 2;
const STARTED: u8 = 4;
const DESTROYED: u8 = 8;
impl Callback {
    fn bit(self) -> u16 {
        1 << self as u8
    }
}
pub(super) fn register(
    w: &mut DisunityWorld,
    c: u64,
    callbacks: Vec<Callback>,
) -> Result<Value, String> {
    live_script(w, c)?;
    if eligibility(w, c).2 {
        return Err("cannot register pending destruction".into());
    }
    if callbacks.len() > 8 {
        return Err("callback metadata budget exceeded".into());
    }
    let mask = callbacks.iter().fold(0, |mask, cb| mask | cb.bit());
    if callbacks.len() != mask.count_ones() as usize {
        return Err("duplicate callback metadata".into());
    }
    if let Some(s) = w.readiness.scripts.get(&c) {
        if s.mask != mask {
            return Err("callback metadata is immutable".into());
        }
        return Ok(super::empty());
    }
    if w.readiness.scripts.len() >= LIMIT {
        return Err("script readiness budget exceeded".into());
    }
    let immediate_parent = super::nested::registration_parent(w, c)?;
    w.readiness.scripts.insert(
        c,
        Script {
            mask,
            flags: 0,
            last_phase: 0,
            immediate_parent,
        },
    );
    Ok(super::empty())
}
fn candidate(w: &DisunityWorld, c: u64, phase: Phase, lane: u8) -> Option<Callback> {
    let s = &w.readiness.scripts[&c];
    let (active, enabled, dying) = eligibility(w, c);
    let dying = dying || phase == Phase::Cleanup;
    let flag = |bit| s.flags & bit != 0;
    match lane {
        0 if !dying && active && !flag(AWAKE) => Some(Callback::Awake),
        1 if flag(ENABLED) && (dying || !active || !enabled) => Some(Callback::OnDisable),
        1 if !dying && active && enabled && flag(AWAKE) && !flag(ENABLED) => {
            Some(Callback::OnEnable)
        }
        2 if dying && flag(AWAKE) && !flag(DESTROYED) => Some(Callback::OnDestroy),
        3 if !dying
            && active
            && enabled
            && flag(ENABLED)
            && !flag(STARTED)
            && phase_method(phase).is_some() =>
        {
            Some(Callback::Start)
        }
        4 if !dying && active && enabled && flag(STARTED) && s.last_phase != w.readiness.root => {
            phase_method(phase)
        }
        _ => None,
    }
}
fn phase_method(phase: Phase) -> Option<Callback> {
    match phase {
        Phase::FixedUpdate => Some(Callback::FixedUpdate),
        Phase::Update => Some(Callback::Update),
        Phase::LateUpdate => Some(Callback::LateUpdate),
        _ => None,
    }
}
pub(super) fn select(
    w: &mut DisunityWorld,
    phase: Phase,
    lanes: std::ops::Range<u8>,
) -> Result<Option<(u64, Callback)>, String> {
    loop {
        let found = lanes.clone().find_map(|lane| {
            w.readiness
                .scripts
                .keys()
                .find_map(|c| candidate(w, *c, phase, lane).map(|cb| (*c, cb)))
        });
        let Some((c, cb)) = found else {
            return Ok(None);
        };
        if w.readiness.scripts[&c].mask & cb.bit() != 0 {
            return Ok(Some((c, cb)));
        }
        spend(w)?;
        consume(w, c, cb);
    }
}
pub(super) fn eligible(w: &DisunityWorld, c: u64, cb: Callback, phase: Phase) -> bool {
    (0..5).any(|lane| candidate(w, c, phase, lane) == Some(cb))
}
pub(super) fn consume(w: &mut DisunityWorld, c: u64, cb: Callback) {
    let s = w.readiness.scripts.get_mut(&c).unwrap();
    match cb {
        Callback::Awake => s.flags |= AWAKE,
        Callback::OnEnable => s.flags |= ENABLED,
        Callback::Start => s.flags |= STARTED,
        Callback::OnDisable => s.flags &= !ENABLED,
        Callback::OnDestroy => s.flags |= DESTROYED,
        _ => s.last_phase = w.readiness.root,
    }
}

pub(super) fn cleanup_pending(w: &DisunityWorld) -> bool {
    w.readiness
        .scripts
        .values()
        .any(|s| s.flags & ENABLED != 0 || (s.flags & AWAKE != 0 && s.flags & DESTROYED == 0))
}

pub(super) fn cleanup_count(w: &DisunityWorld) -> usize {
    w.readiness
        .scripts
        .values()
        .filter(|s| s.flags & ENABLED != 0 || (s.flags & AWAKE != 0 && s.flags & DESTROYED == 0))
        .count()
}

pub(super) fn immediate(
    w: &mut DisunityWorld,
    phase: Phase,
    parent: u64,
) -> Result<Option<(u64, Callback)>, String> {
    loop {
        let found = (0..2).find_map(|lane| {
            w.readiness
                .scripts
                .iter()
                .filter(|(_, s)| s.immediate_parent == Some(parent))
                .filter_map(|(c, _)| {
                    let cb = candidate(w, *c, phase, lane)?;
                    matches!(cb, Callback::Awake | Callback::OnEnable)
                        .then(|| (hierarchy_key(w, *c), *c, cb))
                })
                .min_by(|a, b| a.0.cmp(&b.0))
                .map(|(_, c, cb)| (c, cb))
        });
        let Some((c, cb)) = found else {
            return Ok(None);
        };
        super::nested::admit(w)?;
        if w.readiness.scripts[&c].mask & cb.bit() != 0 {
            return Ok(Some((c, cb)));
        }
        spend(w)?;
        consume(w, c, cb);
    }
}

fn hierarchy_key(w: &DisunityWorld, component: u64) -> (Vec<u64>, u64) {
    let c = live_script(w, component).unwrap();
    let mut path = Vec::new();
    let mut object = Some(c.object);
    while let Some(id) = object {
        path.push(id);
        object = w.metadata[&id].parent;
    }
    path.reverse();
    (path, component & 0xffff_ffff)
}
