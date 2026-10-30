//! The host side of restoring a heap imported from another engine's save.
//!
//! A host that reads an original engine's saved Lua heap decodes it into `lua_resume`'s
//! `SavedHeap` and `SavedThread` and imports it with `lua_resume::import`, which gives one
//! `Snapshot`. This module restores that snapshot against the chunks the game's scripts loaded
//! into this state, and lists the refusals first when the host wants to see every gap at once.
//!
//! Everything here is for the host. The Lua state, the debug capability and the restored
//! threads are never reachable from a script, and a script cannot name a permanent or restore a
//! snapshot.

use std::sync::atomic::Ordering;

use lua_resume::capture::{PermanentError, Permanents, Snapshot};
use lua_resume::restore::{Refusal, Restored, Sources, check_restore};
use mlua::Value;

use crate::error::ScriptError;
use crate::original::GameScripts;
use crate::persist::{permanent_names, refused};

/// The values an imported save may name, besides the standard library and the natives.
#[derive(Clone, Debug, Default)]
pub struct PermanentSpec {
    /// Global paths such as `Classes`. Each path and every function and table reachable from it
    /// through string keys is a permanent, named `permanent <path>` by its shortest path.
    pub paths: Vec<String>,
    /// Values the host names itself, such as the labels an original game gathers from its own
    /// Lua for the tables and functions it keeps out of its saves. A value that already has a
    /// name gets this one as a second name.
    pub extra: Vec<(String, Value)>,
}

impl GameScripts {
    /// The permanents an imported save may name: the standard library, the natives by path
    /// (`native <path>`), the permanents under `spec.paths` and `spec.extra`.
    ///
    /// Host-only: scripts cannot reach this table of names.
    pub fn permanents(&self, spec: &PermanentSpec) -> Result<Permanents, ScriptError> {
        let paths: Vec<&str> = spec.paths.iter().map(String::as_str).collect();
        let names = permanent_names(self.lua(), self.natives(), &paths)?;
        let mut permanents = self.permanents_for(&names, &[]);
        for (name, value) in &spec.extra {
            add_extra(&mut permanents, name, value.clone())?;
        }
        Ok(permanents)
    }

    /// The normalized chunks every script loaded into this state, which a restore generates
    /// code from. Host-only.
    pub fn restore_sources(&self) -> Sources {
        self.loader().lock().expect("loader").sources()
    }

    /// Lists every reason `restore_imported` would refuse `snapshot`, without restoring
    /// anything. The list is empty when nothing is refused up front.
    pub fn check_imported(&self, snapshot: &Snapshot, permanents: &Permanents) -> Vec<Refusal> {
        self.instructions().store(0, Ordering::Relaxed);
        check_restore(
            self.lua(),
            self.debug(),
            snapshot,
            &self.restore_sources(),
            permanents,
        )
    }

    /// Restores a snapshot made by `lua_resume::import` against the chunks loaded in this
    /// state. The result holds the snapshot's roots by name and its threads, in the order the
    /// import gave them, as suspended coroutines the host can adopt into a scheduler. Code is
    /// never in the snapshot, so a restored thread runs the scripts loaded now.
    pub fn restore_imported(
        &self,
        snapshot: &Snapshot,
        permanents: &Permanents,
    ) -> Result<Restored, ScriptError> {
        self.restore_graph(snapshot, permanents)
    }
}

/// Names `value` `name`, as a second name when it is already named.
fn add_extra(permanents: &mut Permanents, name: &str, value: Value) -> Result<(), ScriptError> {
    match permanents.insert(name, value) {
        Ok(()) => Ok(()),
        Err(PermanentError::AlreadyNamed(existing)) => permanents
            .alias(name, &existing)
            .map_err(|error| refused(format!("cannot name {name}: {error}"))),
        Err(error) => Err(refused(format!("cannot name {name}: {error}"))),
    }
}
