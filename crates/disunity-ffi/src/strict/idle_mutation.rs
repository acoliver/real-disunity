use super::{Host, Phase, Stage};
use crate::DisunityWorld;
use serde::Deserialize;
use serde_json::Value;

pub(crate) const IDLE_PROFILE: &str = "native_host_idle_rename_v1";

#[derive(Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub(crate) enum IdleRename {
    #[serde(rename = "object.set")]
    Rename { object: u64, name: String },
}

pub(crate) fn mutate_idle(
    w: &mut DisunityWorld,
    world: u64,
    host: &Host,
    request: IdleRename,
) -> Result<Value, String> {
    let authority = w
        .strict
        .as_ref()
        .ok_or("host mutation requires strict session")?;
    authority.authenticate(host)?;
    if authority.world != world || authority.profile != IDLE_PROFILE {
        return Err("host mutation requires the selected world profile".into());
    }
    if authority.stage != Stage::Idle || authority.active.is_some() || authority.terminal.is_some()
    {
        return Err("host mutation requires initialized Idle without a phase lease".into());
    }
    crate::readiness::require_drained(w, Phase::Initialize)?;
    let IdleRename::Rename { object, name } = request;
    w.set_metadata(object, Some(name), None, None, None)?;
    w.object_snapshot(object)
}
