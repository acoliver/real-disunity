//! The profile for a game's own scripts, beside the strict profile for engine-native scripts.
//!
//! Original scripts expect the API their engine exposed, so the game registers those functions
//! as natives. Each source of scripts (the game's own files, its patches, each mod) runs in its
//! own environment holding only the natives the game granted that source. Everything else a
//! source reads or writes goes to one shared global namespace, as in the original engine: game
//! files define globals other files and mods use. The shared namespace holds the standard
//! library and never a native, so code compiled with `load` sees no natives.
//!
//! Capabilities bound direct access. A function keeps the environment of the source that defined
//! it, so a mod calling a game function runs game code with the game's natives, and a source may
//! pass a native it holds to others. Sources share one namespace and can replace each other's
//! functions, as they could in the original engine.
//!
//! The instruction budget covers coroutines too: see [`crate::coroutines`].

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use mlua::{ChunkMode, FromLuaMulti, Function, IntoLuaMulti, Lua, StdLib, Table, Value};

use crate::config::ScriptConfig;
use crate::coroutines::Budget;
use crate::error::ScriptError;
use crate::host::DEFAULT_MEMORY_LIMIT;
use crate::patches::Overlay;
use crate::sandbox::{lua_value_to_json, restrict_base};

/// One Lua state running a game's original scripts.
pub struct GameScripts {
    lua: Lua,
    config: ScriptConfig,
    instructions: Arc<AtomicU64>,
    natives: BTreeMap<String, Function>,
    sources: BTreeMap<String, Table>,
    scheduler: crate::threads::Shared,
    /// Marks a scheduler sleep among a thread's yields; scripts cannot obtain it.
    sleep_token: Table,
    budget: Budget,
}

impl GameScripts {
    /// The standard library adds `coroutine` to the strict profile's `string`, `table`, `math`
    /// and `utf8`, since original scripts commonly use it.
    pub fn new(config: ScriptConfig) -> Result<Self, ScriptError> {
        let lua = Lua::new_with(
            StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8 | StdLib::COROUTINE,
            mlua::LuaOptions::default(),
        )?;
        lua.set_memory_limit(DEFAULT_MEMORY_LIMIT)?;
        restrict_base(&lua)?;
        let instructions = Arc::new(AtomicU64::new(0));
        let budget = Budget::new(config.instruction_budget, Arc::clone(&instructions));
        budget.hook_main(&lua);
        crate::coroutines::install(&lua, &budget)?;
        let sleep_token = lua.create_table()?;
        Ok(Self {
            lua,
            config,
            instructions,
            natives: BTreeMap::new(),
            sources: BTreeMap::new(),
            scheduler: crate::threads::Shared::default(),
            sleep_token,
            budget,
        })
    }

    /// Registers a native under a path such as `GetMapName` or `terrain.GetHeight`. Natives
    /// are registered before any source is added, so every source's grants are known up front.
    pub fn register_native<A, R, F>(&mut self, path: &str, native: F) -> Result<(), ScriptError>
    where
        A: FromLuaMulti,
        R: IntoLuaMulti,
        F: Fn(&Lua, A) -> mlua::Result<R> + Send + 'static,
    {
        let function = self.lua.create_function(native)?;
        self.register_function(path, function)
    }

    pub(crate) fn register_function(
        &mut self,
        path: &str,
        function: Function,
    ) -> Result<(), ScriptError> {
        if !self.sources.is_empty() {
            return Err(refused(format!(
                "native {path} registered after sources were added"
            )));
        }
        segments(path)?;
        if self.natives.contains_key(path) {
            return Err(refused(format!("native {path} is registered twice")));
        }
        let nested = self.natives.keys().find(|other| {
            other.starts_with(&format!("{path}.")) || path.starts_with(&format!("{other}."))
        });
        if let Some(other) = nested {
            return Err(refused(format!(
                "native {path} nests with native {other}; a path is a function or a namespace"
            )));
        }
        self.natives.insert(path.into(), function);
        Ok(())
    }

    /// Adds a source of scripts that may call exactly the natives in `grants`.
    pub fn add_source(&mut self, name: &str, grants: &[&str]) -> Result<(), ScriptError> {
        if self.sources.contains_key(name) {
            return Err(refused(format!("source {name} is added twice")));
        }
        // The natives at each namespace level, by path; the root is the source's environment.
        let mut levels = BTreeMap::from([(Vec::new(), self.lua.create_table()?)]);
        for &grant in grants {
            let native = self.natives.get(grant).ok_or_else(|| {
                refused(format!("source {name} is granted unknown native {grant}"))
            })?;
            grant_native(&self.lua, &mut levels, &segments(grant)?, native)?;
        }
        let env = forwarding(&self.lua, Vec::new(), levels[&Vec::new()].clone())?;
        self.sources.insert(name.into(), env);
        Ok(())
    }

    /// Runs one script file as `source`. Only source text is accepted.
    pub fn run(&mut self, source: &str, file: &str, text: &[u8]) -> Result<(), ScriptError> {
        let env = self.environment(source)?;
        self.instructions.store(0, Ordering::Relaxed);
        let chunk = self
            .lua
            .load(text)
            .set_name(file)
            .set_mode(ChunkMode::Text)
            .set_environment(env);
        chunk.exec().map_err(|error| self.error(file, error))
    }

    /// Runs one of the game's original files as `source` after applying its patches, and
    /// returns the applied patch names.
    pub fn run_patched(
        &mut self,
        source: &str,
        file: &str,
        original: &[u8],
        overlay: &Overlay,
    ) -> Result<Vec<String>, ScriptError> {
        let patched = overlay.apply(file, original)?;
        self.run(source, file, &patched.source)?;
        Ok(patched.applied)
    }

    /// Calls the function at `path` in the shared namespace, as the engine calls a game's
    /// entry points. The function runs with the environment of the source that defined it.
    pub fn call<R: FromLuaMulti>(
        &mut self,
        path: &str,
        args: impl IntoLuaMulti,
    ) -> Result<R, ScriptError> {
        let function = match lookup(&self.lua, &segments(path)?)? {
            Value::Function(function) => function,
            other => {
                return Err(refused(format!(
                    "{path} is not a function (found {})",
                    other.type_name()
                )));
            }
        };
        self.instructions.store(0, Ordering::Relaxed);
        function.call(args).map_err(|error| self.error(path, error))
    }

    /// Evaluates `code` as `source` and returns its first result as JSON, for tools and tests.
    pub fn eval_json(
        &mut self,
        source: &str,
        code: &str,
    ) -> Result<serde_json::Value, ScriptError> {
        let env = self.environment(source)?;
        self.instructions.store(0, Ordering::Relaxed);
        let chunk = self
            .lua
            .load(code)
            .set_name("<eval>")
            .set_mode(ChunkMode::Text)
            .set_environment(env);
        let value: Value = chunk.eval().map_err(|error| self.error("<eval>", error))?;
        Ok(lua_value_to_json(&value))
    }

    /// Caps the memory the Lua state may hold, in bytes.
    pub fn set_memory_limit(&mut self, bytes: usize) -> Result<(), ScriptError> {
        if bytes == 0 {
            return Err(refused("a memory limit must be positive".into()));
        }
        self.lua.set_memory_limit(bytes)?;
        Ok(())
    }

    pub(crate) fn lua(&self) -> &Lua {
        &self.lua
    }

    pub(crate) fn scheduler(&self) -> &crate::threads::Shared {
        &self.scheduler
    }

    pub(crate) fn sleep_token(&self) -> &Table {
        &self.sleep_token
    }

    pub(crate) fn instructions(&self) -> &Arc<AtomicU64> {
        &self.instructions
    }

    pub(crate) fn budget(&self) -> &Budget {
        &self.budget
    }

    pub(crate) fn natives(&self) -> &BTreeMap<String, Function> {
        &self.natives
    }

    fn environment(&self, source: &str) -> Result<Table, ScriptError> {
        self.sources
            .get(source)
            .cloned()
            .ok_or_else(|| refused(format!("unknown source {source}")))
    }

    fn error(&self, chunk: &str, error: mlua::Error) -> ScriptError {
        if error.to_string().contains("instruction budget") {
            ScriptError::BudgetExceeded {
                chunk: chunk.into(),
                budget: self.config.instruction_budget,
            }
        } else {
            ScriptError::from(error)
        }
    }
}

/// The identifiers of a dotted path, each a Lua name.
pub(crate) fn segments(path: &str) -> Result<Vec<String>, ScriptError> {
    let names: Vec<String> = path.split('.').map(str::to_owned).collect();
    let valid = names.iter().all(|name| {
        let mut bytes = name.bytes();
        bytes
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
    });
    if valid {
        Ok(names)
    } else {
        Err(refused(format!("{path:?} is not a dotted Lua name")))
    }
}

/// Records `native` at `path`, creating a namespace view for each level above it.
fn grant_native(
    lua: &Lua,
    levels: &mut BTreeMap<Vec<String>, Table>,
    path: &[String],
    native: &Function,
) -> Result<(), ScriptError> {
    for depth in 1..path.len() {
        let prefix = path[..depth].to_vec();
        if !levels.contains_key(&prefix) {
            let natives = lua.create_table()?;
            let view = forwarding(lua, prefix.clone(), natives.clone())?;
            levels[&path[..depth - 1]].raw_set(path[depth - 1].as_str(), view)?;
            levels.insert(prefix, natives);
        }
    }
    let (last, parent) = path.split_last().expect("paths have a segment");
    levels[parent].raw_set(last.as_str(), native.clone())?;
    Ok(())
}

/// A view of the shared namespace at `path` that holds no keys itself. Reads find a granted
/// native in `natives` first, then the shared value; writes always go to the shared namespace.
/// Natives therefore stay visible however the game rebinds or extends the namespace, and the
/// game's own entries show through.
fn forwarding(lua: &Lua, path: Vec<String>, natives: Table) -> Result<Table, ScriptError> {
    let meta = lua.create_table()?;
    let read = path.clone();
    meta.raw_set(
        "__index",
        lua.create_function(move |lua, (_, key): (Table, Value)| {
            let native: Value = natives.raw_get(key.clone())?;
            if !native.is_nil() {
                return Ok(native);
            }
            match lookup(lua, &read)? {
                Value::Table(shared) => shared.get::<Value>(key),
                _ => Ok(Value::Nil),
            }
        })?,
    )?;
    meta.raw_set(
        "__newindex",
        lua.create_function(move |lua, (_, key, value): (Table, Value, Value)| {
            match lookup(lua, &path)? {
                Value::Table(shared) => shared.set(key, value),
                _ => Err(mlua::Error::RuntimeError(format!(
                    "cannot assign into {}: it is not a table",
                    path.join(".")
                ))),
            }
        })?,
    )?;
    let table = lua.create_table()?;
    table.set_metatable(Some(meta));
    Ok(table)
}

/// The value at `path` in the shared namespace, or nil where the path stops.
pub(crate) fn lookup(lua: &Lua, path: &[String]) -> mlua::Result<Value> {
    let mut value = Value::Table(lua.globals());
    for name in path {
        value = match value {
            Value::Table(table) => table.get(name.as_str())?,
            _ => return Ok(Value::Nil),
        };
    }
    Ok(value)
}

fn refused(message: String) -> ScriptError {
    ScriptError::Sandbox { message }
}
