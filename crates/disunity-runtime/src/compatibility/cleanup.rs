use serde_json::json;

use super::CompatibilityHost;
use crate::HostError;

impl CompatibilityHost {
    pub(super) fn unload_strict(&mut self, world: u64) -> Result<(), HostError> {
        self.bridge.require_idle()?;
        if super::callback_batches::selected(self, world)? {
            return self.cleanup_contextual(world);
        }
        let before = self.callback_failures(0)?.total;
        let result = if self.measured_empty()? {
            // SAFETY: an empty bridge has no user callbacks or iterator disposal to execute.
            self.bridge.check(unsafe { (self.bridge.unload)() })
        } else {
            self.cleanup_in_root(world)
        };
        let closed = self.close_phase_session(world);
        let drained = self.measured_empty()?;
        if drained {
            self.loaded = false;
            self.loaded_world = None;
        }
        let result = super::phase::join(result, closed);
        let result = super::phase::join(
            result,
            if drained {
                Ok(())
            } else {
                Err(HostError::Compatibility(
                    "cleanup_incomplete: measured managed roots retained; reload refused".into(),
                ))
            },
        );
        super::phase::join(
            result,
            if self.callback_failures(0)?.total > before {
                self.ensure_no_callback_failures()
            } else {
                Ok(())
            },
        )
    }

    fn cleanup_in_root(&mut self, world: u64) -> Result<(), HostError> {
        let terminal = self.terminal_selected(world);
        let context = if terminal {
            self.terminal_context(world)?
        } else {
            self.phase_context(world, "Cleanup")?
        };
        let called = self.bridge.invoke(
            self.bridge.phase,
            &json!({"phase":"Cleanup","context":context}),
        );
        let finished = if called.is_ok() || self.measured_empty()? {
            if terminal {
                self.control(world, json!({"op":"phase.finish","context":context}))
                    .map(|_| ())
            } else {
                self.finish_attempt(world, &context)
            }
        } else {
            Err(HostError::Compatibility(
                "cleanup_transport_or_execution_incomplete: no callback retry".into(),
            ))
        };
        super::phase::join(called, finished)
    }
}
