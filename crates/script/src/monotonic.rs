//! Explicit absolute monotonic input for source-granted natives.

use std::{io, sync::Arc, time::Duration};

use mlua::{ErrorContext, MultiValue};

use crate::{GameScripts, ScriptError};

/// An absolute sample since a supplier-owned fixed monotonic origin.
///
/// The supplier guarantees nonnegative, nondecreasing samples (equal values are valid).
/// Reads must not advance engine clocks. This crate never acquires a physical source.
pub type MonotonicInput = Arc<dyn Fn() -> io::Result<Duration> + Send + Sync>;

impl GameScripts {
    /// Registers a zero-argument native returning one signed integer in milliseconds.
    ///
    /// Registration follows native path/duplicate/lifecycle rules, then validates and
    /// discards one real sample. Each valid call reads once and floors fractional
    /// milliseconds. Values above `i64::MAX` milliseconds and input errors fail explicitly.
    /// Zero and the supplier's origin are preserved; no sample is cached or rebased.
    pub fn register_monotonic_millis(
        &mut self,
        name: &str,
        input: MonotonicInput,
    ) -> Result<(), ScriptError> {
        self.check_native_registration(name)?;
        read_millis(&input).map_err(|error| {
            ScriptError::from(
                mlua::Error::external(error)
                    .context(format!("native {name}: validating monotonic input")),
            )
        })?;
        let operation = format!("native {name}: reading monotonic input");
        self.register_native(name, move |_, args: MultiValue| {
            if !args.is_empty() {
                return Err(mlua::Error::RuntimeError(format!(
                    "{operation}: expected zero arguments"
                )));
            }
            read_millis(&input)
                .map_err(|error| mlua::Error::external(error).context(operation.clone()))
        })
    }
}

fn read_millis(input: &MonotonicInput) -> io::Result<i64> {
    i64::try_from(input()?.as_millis()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "monotonic sample exceeds i64::MAX milliseconds",
        )
    })
}
