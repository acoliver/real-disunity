//! Reports which per-frame messages a script defines.
use crate::{DisunityResult, callbacks::mutate};
use worldrunner_world_simulation::ScriptMessages;

/// Records the per-frame messages (bit 0 `FixedUpdate`, bit 1 `Update`, bit 2 `LateUpdate`) that
/// the script bound to `identity` defines. The script then receives only those messages. A script
/// never reported receives all three, and a clone starts with its source's set.
#[unsafe(no_mangle)]
pub extern "C" fn disunity_script_messages(
    world: u64,
    identity: u64,
    messages: u32,
) -> DisunityResult {
    let Some(messages) = ScriptMessages::from_bits(messages) else {
        return DisunityResult::invalid_argument();
    };
    mutate(world, |session| {
        let script = session
            .dispatch
            .scripts
            .handle_of(identity)
            .ok_or("script binding missing")?;
        session
            .native
            .set_script_messages(script, messages)
            .map_err(crate::errors::AbiError::from)
    })
}
