use serde::Deserialize;
use serde_json::{Value, json, value::RawValue};

use super::{CompatibilityHost, HostError, invalid, selected};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Response<'a> {
    version: u32,
    ok: bool,
    #[serde(borrow)]
    data: &'a RawValue,
}

fn batch_bytes(response: &str) -> Result<Option<&[u8]>, HostError> {
    let response: Response<'_> =
        serde_json::from_str(response).map_err(|_| invalid("invalid native batch response"))?;
    if response.version != 1 || !response.ok {
        return Err(invalid("incompatible native batch response"));
    }
    let bytes = response.data.get().as_bytes();
    if bytes == b"null" {
        return Ok(None);
    }
    if bytes.is_empty() || bytes.len() > 1024 * 1024 {
        return Err(invalid("native batch exceeds the one MiB envelope limit"));
    }
    Ok(Some(bytes))
}

impl CompatibilityHost {
    pub(super) fn drain_contextual_batches(
        &self,
        world: u64,
        context: &Value,
    ) -> Result<(), HostError> {
        let claim = self.native.request(
            world,
            &json!({"op":"phase.claim","phase":"Cleanup","context":context}),
        )?;
        if claim["execute"] != true {
            return Err(invalid("native Cleanup claim denied execution"));
        }
        for _ in 0..=4096 {
            let response = self
                .native
                .request_text(world, &json!({"op":"callbacks.next","context":context}))?;
            let Some(bytes) = batch_bytes(&response)? else {
                return Ok(());
            };
            let _dispatch = self.bridge.enter_dispatch()?;
            // SAFETY: the owned native envelope is bounded, valid UTF-8 and borrowed
            // only for this synchronous call. A failed call is never replayed here.
            let code = unsafe { (self.bridge.phase)(bytes.as_ptr(), bytes.len()) };
            self.bridge.check(code)?;
        }
        Err(invalid(
            "native callback issuance did not terminate within its limit",
        ))
    }

    pub(super) fn finalize_contextual_cleanup(
        &mut self,
        world: u64,
        context: &Value,
    ) -> Result<(), HostError> {
        self.control(world, json!({"op":"phase.finish","context":context}))?;
        let status = self
            .native
            .request(world, &json!({"op":"session.status"}))?;
        if status["stage"] != "Closed" {
            return Err(invalid("Cleanup finished without permanent Closed status"));
        }
        self.bridge.require_idle()?;
        // SAFETY: this route has completed native obligations. The contextual
        // bridge finalizer releases bindings without replaying user callbacks.
        self.bridge.check(unsafe { (self.bridge.unload)() })?;
        if !self.measured_empty()? {
            return Err(invalid(
                "cleanup_incomplete: measured managed roots retained",
            ));
        }
        self.loaded = false;
        self.loaded_world = None;
        Ok(())
    }

    /// Drains native-selected Cleanup records, preserving ownership on uncertainty.
    pub fn cleanup_contextual(&mut self, world: u64) -> Result<(), HostError> {
        self.bridge.require_idle()?;
        if self.loaded_world != Some(world) || !selected(self, world)? {
            return Err(invalid("Cleanup requires the loaded contextual world"));
        }
        let before = self.callback_failures(0)?.total;
        let context = self.phase_context(world, "Cleanup")?;
        self.drain_contextual_batches(world, &context)?;
        self.finalize_contextual_cleanup(world, &context)?;
        if self.callback_failures(0)?.total > before {
            return self.ensure_no_callback_failures();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_batch_projection_preserves_native_tokens_and_null() {
        let response =
            "{\"version\":1,\"ok\":true,\"data\":{\"id\":\"18446744073709551615\",\"delta\":-0.0}}";
        assert_eq!(
            batch_bytes(response).unwrap().unwrap(),
            b"{\"id\":\"18446744073709551615\",\"delta\":-0.0}"
        );
        assert!(
            batch_bytes("{\"version\":1,\"ok\":true,\"data\":null}")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn raw_batch_response_rejects_duplicate_unknown_and_wrong_envelope() {
        for response in [
            "{\"version\":1,\"ok\":true,\"data\":null,\"data\":null}",
            "{\"version\":1,\"ok\":true,\"data\":null,\"unknown\":0}",
            "{\"version\":2,\"ok\":true,\"data\":null}",
            "{\"version\":1,\"ok\":false,\"data\":null}",
        ] {
            assert!(batch_bytes(response).is_err());
        }
    }
}
