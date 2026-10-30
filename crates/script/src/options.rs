//! How a host has the loader prepare the chunks of a game's scripts.
//!
//! Every option is off by default, so a host that sets none gets stock Lua semantics, strict
//! refusal of chunks the rewrite cannot make resumable, and an error for a reused chunk name.
//! The division and policy options apply to every text chunk: the game's files, patched files,
//! and the text scripts pass to `load`.

use std::collections::BTreeMap;

use lua_resume::import::ImportSources;
use lua_resume::syntax::OperatorHooks;

use crate::config::{DivisionSemantics, NameCollisions, RefusedChunks};
use crate::division::{DIVIDE_HOOK, DIVIDE_PERMANENT, create};
use crate::error::ScriptError;
use crate::original::GameScripts;

impl GameScripts {
    /// Selects how `/` divides in every chunk loaded afterwards, restore continuations
    /// included. [`DivisionSemantics::IntegerTruncating`] installs an internal function that
    /// the loaded text calls in place of the operator; scripts see it as the global `__lr_div`,
    /// which holds no capability.
    ///
    /// It must be called before any chunk loads, since text already loaded keeps the operator
    /// it was printed with.
    pub fn set_division(&mut self, division: DivisionSemantics) -> Result<(), ScriptError> {
        if !self.loader().lock().expect("loader").is_empty() {
            return Err(ScriptError::Sandbox {
                message: "division semantics must be set before any chunk is loaded".into(),
            });
        }
        let hooks = match division {
            DivisionSemantics::Stock => OperatorHooks::none(),
            DivisionSemantics::IntegerTruncating => {
                let function = create(self.lua())?;
                self.lua()
                    .globals()
                    .raw_set(DIVIDE_HOOK, function.clone())?;
                self.standard_mut()
                    .insert(DIVIDE_PERMANENT, mlua::Value::Function(function))
                    .map_err(|error| ScriptError::Sandbox {
                        message: error.to_string(),
                    })?;
                OperatorHooks::none()
                    .with_divide(DIVIDE_HOOK)
                    .map_err(|error| ScriptError::Sandbox {
                        message: error.to_string(),
                    })?
            }
        };
        self.loader().lock().expect("loader").set_hooks(hooks);
        Ok(())
    }

    /// Chooses whether a chunk the resumable rewrite refuses fails to load (the default) or
    /// loads as plain text. See [`GameScripts::plain_chunks`].
    pub fn set_refused_chunks(&mut self, policy: RefusedChunks) {
        self.loader().lock().expect("loader").set_refused(policy);
    }

    /// Chooses whether loading a different text under a chunk name already used is an error
    /// (the default) or loads the new text as plain text, leaving the first chunk's tree in
    /// place.
    pub fn set_name_collisions(&mut self, policy: NameCollisions) {
        self.loader().lock().expect("loader").set_collisions(policy);
    }

    /// The chunks that loaded as plain text, by chunk name, with the reason. Threads running
    /// their code cannot be saved. Empty unless a policy allowed plain loads.
    pub fn plain_chunks(&self) -> BTreeMap<String, String> {
        self.loader().lock().expect("loader").plain()
    }

    /// The rewritten chunks loaded so far, by the source name a save made by the original
    /// build gives them (`@path` for a chunk loaded as `=path`), for importing such a save.
    /// Host only: scripts cannot reach it.
    pub fn import_sources(&self) -> ImportSources {
        self.loader().lock().expect("loader").import_sources()
    }
}
