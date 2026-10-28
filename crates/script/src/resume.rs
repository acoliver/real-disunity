//! Saving and restoring a scheduler thread where it stopped, through `lua-resume`.
//!
//! Every chunk the scripts load is rewritten first (see [`crate::load`]), which is what makes a
//! suspended thread readable: [`GameScripts::capture_thread`] turns its frames, locals and
//! closures into data, and [`GameScripts::restore_thread`] builds the frames again from the
//! rewritten source of the game's own chunks. Code is never saved, so a restored thread runs the
//! current scripts.
//!
//! Values the save refers to by name are the permanents: the standard library, the natives by
//! path, the game's permanent tables and functions, the tables of the saved data (so threads
//! and data keep sharing them) and the scheduler's sleep token. A thread that holds another
//! coroutine, or a table the saved data does not reach and another thread also holds, is not
//! supported yet: the first is refused, the second is saved once per thread.

use std::sync::atomic::Ordering;

use lua_resume::capture::{Permanents, Snapshot, capture};
use lua_resume::restore::restore;
use mlua::{Table, Thread, Value};

use crate::error::ScriptError;
use crate::original::GameScripts;
use crate::persist::Saved;

impl GameScripts {
    /// The names a thread's save may refer to. `named` holds the natives and permanents by
    /// their saved names, and `tables` the saved data's tables by id.
    pub(crate) fn thread_permanents<'a>(
        &self,
        named: impl Iterator<Item = (&'a Saved, &'a Value)>,
        tables: &[Table],
    ) -> Result<Permanents, ScriptError> {
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
        for (saved, value) in named {
            match saved {
                Saved::Native(path) => add(format!("native {path}"), value.clone()),
                Saved::Permanent(path) => add(format!("permanent {path}"), value.clone()),
                _ => {}
            }
        }
        for (id, table) in tables.iter().enumerate() {
            add(format!("data table {id}"), Value::Table(table.clone()));
        }
        Ok(permanents)
    }

    /// Reads the suspended coroutine of scheduler thread `index`.
    pub(crate) fn capture_thread(
        &self,
        thread: &Thread,
        permanents: &Permanents,
        index: usize,
    ) -> Result<Snapshot, ScriptError> {
        self.instructions().store(0, Ordering::Relaxed);
        let snapshot = capture(self.lua(), self.debug(), thread, permanents)
            .map_err(|error| refused(format!("cannot save scheduler thread {index}: {error}")))?;
        if snapshot.threads.len() != 1 {
            return Err(refused(format!(
                "cannot save scheduler thread {index}: it holds another coroutine, which is not supported yet"
            )));
        }
        Ok(snapshot)
    }

    /// Rebuilds a coroutine suspended where `snapshot` says scheduler thread `index` stopped.
    pub(crate) fn restore_thread(
        &self,
        snapshot: &Snapshot,
        permanents: &Permanents,
        index: usize,
    ) -> Result<Thread, ScriptError> {
        self.instructions().store(0, Ordering::Relaxed);
        let sources = self.loader().lock().expect("loader").sources();
        restore(self.lua(), self.debug(), snapshot, &sources, permanents)
            .map_err(|error| refused(format!("cannot restore scheduler thread {index}: {error}")))
    }
}

fn refused(message: String) -> ScriptError {
    ScriptError::Sandbox { message }
}
