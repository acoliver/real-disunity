//! Host input into the same native world managed `Input` reads.
use std::time::Duration;

use crate::{DisunityHost, HostError, native};

impl DisunityHost {
    /// Queues a key transition at `timestamp` of simulated time. `key` is the managed
    /// `KeyCode` value. Timestamps must not decrease and must follow the last advanced frame.
    pub fn push_key(
        &mut self,
        timestamp: Duration,
        key: u16,
        pressed: bool,
    ) -> Result<(), HostError> {
        let world = self.world.ok_or(HostError::NoWorld)?;
        type Push = unsafe extern "C" fn(u64, u64, u64, u32, u32, f32) -> native::DisunityResult;
        // The image stays loaded for the host's lifetime; its scalar ABI is generated.
        let push: Push = unsafe {
            *self
                ._library
                .get(b"disunity_input_push\0")
                .map_err(|_| HostError::MissingSymbol("disunity_input_push".into()))?
        };
        let nanos = u64::try_from(timestamp.as_nanos()).map_err(|_| HostError::Abi(2))?;
        let value = if pressed { 1.0 } else { 0.0 };
        let result = unsafe { push(world, nanos, 1_000_000_000, 0, u32::from(key), value) };
        if result.code == 0 {
            Ok(())
        } else {
            Err(self.native_error(result))
        }
    }
}
