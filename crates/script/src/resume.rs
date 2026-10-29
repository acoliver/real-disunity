//! Saving and restoring all of a game's Lua state as one graph, through `lua-resume`.
//!
//! Every chunk the scripts load is rewritten first (see [`crate::load`]), which is what makes a
//! suspended thread readable. [`GameScripts::capture_graph`] turns the saved roots and every
//! scheduler thread that has run, with their frames, locals and closures, into one
//! [`Snapshot`], so a table, closure or upvalue cell that threads and roots share is saved once.
//! [`GameScripts::restore_graph`] builds the roots and frames again from the rewritten source of
//! the game's own chunks. Code is never saved, so a restored thread runs the current scripts.
//!
//! Values the save refers to by name are the permanents: the standard library, the natives by
//! path, the game's permanent tables and functions, the scheduler's sleep token and the
//! scheduler threads that have not run yet.

use std::sync::atomic::Ordering;

use lua_resume::capture::{Permanents, Snapshot, capture_all};
use lua_resume::restore::{Restored, restore_all};
use mlua::{Thread, Value};

use crate::error::ScriptError;
use crate::original::GameScripts;

/// The name a save gives scheduler thread `index` while it has not run, so values that hold it
/// refer to it.
fn thread_name(index: usize) -> String {
    format!("scheduler thread {index}")
}

impl GameScripts {
    /// The names a save may refer to. `named` holds the natives and permanents by their saved
    /// names and `threads` the scheduler threads that stand in for threads that have not run,
    /// by index.
    pub(crate) fn permanents_for(
        &self,
        named: &[(String, Value)],
        threads: &[(usize, Thread)],
    ) -> Permanents {
        let mut permanents = self.standard().clone();
        let mut add = |name: String, value: Value| {
            // A value that already has a name keeps it, on both the save and the restore side.
            let _ = permanents.insert(&name, value);
        };
        add(
            "scheduler sleep token".into(),
            Value::Table(self.sleep_token().clone()),
        );
        for (name, table) in self.environments() {
            add(name.clone(), Value::Table(table.clone()));
        }
        for (name, value) in named {
            add(name.clone(), value.clone());
        }
        for (index, thread) in threads {
            add(thread_name(*index), Value::Thread(thread.clone()));
        }
        permanents
    }

    /// Captures `roots` and the suspended `threads` as one graph.
    pub(crate) fn capture_graph(
        &self,
        roots: &[(&str, Value)],
        threads: &[Thread],
        permanents: &Permanents,
    ) -> Result<Snapshot, ScriptError> {
        self.instructions().store(0, Ordering::Relaxed);
        capture_all(self.lua(), self.debug(), roots, threads, permanents)
            .map_err(|error| refused(format!("cannot save: {error}")))
    }

    /// Rebuilds the roots and coroutines of `snapshot` as one graph.
    pub(crate) fn restore_graph(
        &self,
        snapshot: &Snapshot,
        permanents: &Permanents,
    ) -> Result<Restored, ScriptError> {
        self.instructions().store(0, Ordering::Relaxed);
        let sources = self.loader().lock().expect("loader").sources();
        restore_all(self.lua(), self.debug(), snapshot, &sources, permanents)
            .map_err(|error| refused(format!("cannot restore: {error}")))
    }
}

fn refused(message: String) -> ScriptError {
    ScriptError::Sandbox { message }
}
