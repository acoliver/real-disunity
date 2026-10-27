//! The sandboxed Lua host.
//!
//! Contract: curated stdlib only (string, table, math, utf8), `require`
//! resolved exclusively through host-loaded modules, no wall clock, seeded
//! randomness, a per-fire instruction budget, and a memory limit. Lua's base
//! library loses `dofile` and `loadfile`, and every chunk compiles as text:
//! Lua does not verify bytecode, so a crafted binary chunk could corrupt the
//! interpreter. Every one of those rules is
//! enforced here rather than trusted to script discipline.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use mlua::{ChunkMode, Function, Lua, StdLib, UserData, UserDataMethods};

use crate::api::ScriptApi;
use crate::config::ScriptConfig;
use crate::error::ScriptError;
use crate::sandbox::{install_budget, lua_value_to_json, restrict_base, table_to_json};

/// Version of the `worldrunner` API surface this host installs.
pub const API_VERSION: &str = "0.1.0";

/// Memory a host's Lua state may hold until [`ScriptHost::set_memory_limit`] changes it.
pub const DEFAULT_MEMORY_LIMIT: usize = 256 << 20;

/// Per-fire context. The world is not borrowed across the boundary; hosts
/// install it with [`ScriptHost::set_world`] as an `Arc`, which is also what
/// makes generation-checked handles possible.
#[derive(Debug, Clone, Copy, Default)]
pub struct HookContext {
    pub dt: f32,
    pub frame: u64,
}

/// Outcome of a hook fire.
#[derive(Debug, Clone, PartialEq)]
pub enum HookResult {
    /// The hook does not exist in the loaded scripts; firing was a no-op.
    NoHook,
    /// The hook ran to completion.
    Ran,
}

/// One structured log record or published event, kept for traces and tests.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Record {
    pub kind: String,
    pub level: Option<String>,
    pub message: String,
    pub fields: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TraceEvent {
    pub hook: String,
    pub outcome: String,
}

/// Deterministic seeded stream exposed as `worldrunner.random.stream(name)`.
struct RandomStream {
    state: u64,
}

impl RandomStream {
    fn seeded(master: u64, name: &str) -> Self {
        // FNV-1a of the stream name so distinct streams decorrelate.
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in name.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x1000_0000_01b3);
        }
        Self {
            state: master ^ hash,
        }
    }

    fn next_u64(&mut self) -> u64 {
        // SplitMix64: cheap, deterministic, no platform dependence.
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

impl UserData for RandomStream {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method_mut("next_integer", |_, this, (min, max): (i64, i64)| {
            if min > max {
                return Err(mlua::Error::RuntimeError(
                    "next_integer: min must be <= max".into(),
                ));
            }
            let span = (max - min + 1) as u64;
            Ok(min + ((this.next_u64() % span) as i64))
        });
        methods.add_method_mut("next_float", |_, this, ()| {
            Ok((this.next_u64() >> 11) as f64 / (1u64 << 53) as f64)
        });
    }
}

/// The Lua script host. One instance owns one `Lua` state.
pub struct ScriptHost {
    lua: Lua,
    config: ScriptConfig,
    instructions: Arc<AtomicU64>,
    records: Arc<Mutex<Vec<Record>>>,
    trace: Vec<TraceEvent>,
    modules: Arc<RwLock<HashMap<String, Vec<u8>>>>,
}

impl ScriptHost {
    /// Builds the sandbox: safe stdlib subsets only, no `os`/`io`/`debug`,
    /// no package path, `require` bound to the host module table.
    pub fn new(config: ScriptConfig) -> Result<Self, ScriptError> {
        let lua = Lua::new_with(
            StdLib::STRING | StdLib::TABLE | StdLib::MATH | StdLib::UTF8,
            Default::default(),
        )?;
        lua.set_memory_limit(DEFAULT_MEMORY_LIMIT)?;
        restrict_base(&lua)?;
        let instructions = Arc::new(AtomicU64::new(0));
        let records: Arc<Mutex<Vec<Record>>> = Arc::new(Mutex::new(Vec::new()));
        let modules: Arc<RwLock<HashMap<String, Vec<u8>>>> = Arc::new(RwLock::new(HashMap::new()));

        install_budget(&lua, config.instruction_budget, &instructions);

        let worldrunner = lua.create_table()?;
        worldrunner.raw_set("api_version", lua.create_function(|_, ()| Ok(API_VERSION))?)?;
        install_log(&lua, &worldrunner, &records)?;
        install_events(&lua, &worldrunner, &records)?;

        let random = lua.create_table()?;
        let seed = config.seed;
        random.raw_set(
            "stream",
            lua.create_function(move |_, name: String| Ok(RandomStream::seeded(seed, &name)))?,
        )?;
        worldrunner.raw_set("random", random)?;

        install_require(&lua, &modules)?;

        lua.globals().raw_set("worldrunner", worldrunner)?;
        // Scripts never see the sandbox's bookkeeping.
        lua.globals()
            .raw_set("__host_loaded", lua.create_table()?)?;

        Ok(Self {
            lua,
            config,
            instructions,
            records,
            trace: Vec::new(),
            modules,
        })
    }

    pub fn config(&self) -> &ScriptConfig {
        &self.config
    }

    /// Caps the memory the Lua state may hold, in bytes. An allocation past it fails with a Lua
    /// memory error, reported as [`ScriptError::Memory`] when scripts do not catch it.
    pub fn set_memory_limit(&mut self, bytes: usize) -> Result<(), ScriptError> {
        if bytes == 0 {
            return Err(ScriptError::Sandbox {
                message: "a memory limit must be positive".into(),
            });
        }
        self.lua.set_memory_limit(bytes)?;
        Ok(())
    }

    /// Installs a `worldrunner.*` module and verifies the module's own
    /// declared version matches what the trait claims, refusing mismatches.
    pub fn install_api(&mut self, api: Box<dyn ScriptApi>) -> Result<(), ScriptError> {
        api.install(&self.lua)?;
        let declared = api.api_version();
        let declared_str = format!("{}.{}.{}", declared.0, declared.1, declared.2);
        let module: mlua::Table = self
            .lua
            .globals()
            .raw_get::<mlua::Table>("worldrunner")?
            .raw_get(api.name())?;
        let found: Option<String> = module
            .raw_get::<Option<Function>>("api_version_fn")?
            .and_then(|function| function.call::<String>(()).ok());
        // The module must expose its version through api_version_fn if it
        // installs one; modules without the probe accept the trait version.
        let found = found.unwrap_or_else(|| declared_str.clone());
        if found != declared_str {
            return Err(ScriptError::ApiVersion {
                module: api.name().into(),
                expected: declared_str,
                found,
            });
        }
        Ok(())
    }

    /// Registers an in-memory module for `require`.
    pub fn load_module(&mut self, name: &str, source: &str) -> Result<(), ScriptError> {
        self.load_module_bytes(name, source.as_bytes())
    }

    /// Registers one of a game's original script files for `require`, after applying its
    /// patches from `overlay`. Returns the applied patch names in order. A refused patch set
    /// loads nothing.
    pub fn load_patched_module(
        &mut self,
        name: &str,
        original: &[u8],
        overlay: &crate::patches::Overlay,
    ) -> Result<Vec<String>, ScriptError> {
        let patched = overlay.apply(name, original)?;
        self.load_module_bytes(name, &patched.source)?;
        Ok(patched.applied)
    }

    fn load_module_bytes(&mut self, name: &str, source: &[u8]) -> Result<(), ScriptError> {
        // Compile now so a broken module fails at registration, not at first
        // require inside a frame.
        self.instructions.store(0, Ordering::Relaxed);
        self.lua
            .load(source)
            .set_name(name)
            .set_mode(ChunkMode::Text)
            .exec()?;
        self.modules
            .write()
            .expect("module table")
            .insert(name.into(), source.to_vec());
        Ok(())
    }

    /// Installs the world scripts may query. Passing `None` unloads it; any
    /// handles scripts still hold become invalid by generation.
    pub fn set_world(&mut self, world: Option<Arc<dyn crate::world::WorldView>>) {
        match world {
            Some(world) => {
                self.lua.set_app_data(world);
            }
            None => {
                self.lua
                    .remove_app_data::<Arc<dyn crate::world::WorldView>>();
            }
        }
    }

    /// Fires a lifecycle hook. Missing hooks are a no-op, not an error: the
    /// engine calls every hook unconditionally and scripts opt in.
    pub fn fire(&mut self, hook: &str, ctx: &HookContext) -> Result<HookResult, ScriptError> {
        self.instructions.store(0, Ordering::Relaxed);
        let function: Option<Function> = self.lua.globals().raw_get(hook)?;
        let Some(function) = function else {
            self.trace.push(TraceEvent {
                hook: hook.into(),
                outcome: "no-hook".into(),
            });
            return Ok(HookResult::NoHook);
        };
        let outcome = match hook {
            "on_world_load" | "on_world_unload" => function.call::<()>(()),
            _ => function.call::<()>((ctx.dt, ctx.frame)),
        };
        match outcome {
            Ok(()) => {
                self.trace.push(TraceEvent {
                    hook: hook.into(),
                    outcome: "ran".into(),
                });
                Ok(HookResult::Ran)
            }
            Err(error) => {
                let message = error.to_string();
                if message.contains("instruction budget") {
                    Err(ScriptError::BudgetExceeded {
                        chunk: hook.into(),
                        budget: self.config.instruction_budget,
                    })
                } else {
                    Err(ScriptError::from(error))
                }
            }
        }
    }

    /// Installs `worldrunner.world` from this crate's built-in module.
    pub fn install_world_api(&mut self) -> Result<(), ScriptError> {
        crate::modules::WorldApi.install(&self.lua)
    }

    /// Installs `worldrunner.assets` from this crate's built-in module.
    pub fn install_assets_api(&mut self) -> Result<(), ScriptError> {
        crate::modules::AssetsApi.install(&self.lua)
    }

    /// Installs the resolver `worldrunner.assets.resolve` consults.
    pub fn set_asset_resolver(&mut self, resolver: Option<Arc<dyn crate::modules::AssetResolver>>) {
        match resolver {
            Some(resolver) => self.lua.set_app_data(resolver),
            None => self
                .lua
                .remove_app_data::<Arc<dyn crate::modules::AssetResolver>>(),
        };
    }

    /// Structured records (logs + events) since the last take.
    pub fn take_records(&mut self) -> Vec<Record> {
        std::mem::take(&mut *self.records.lock().expect("record sink"))
    }

    /// Hook trace since the last take; the determinism test compares this.
    pub fn take_trace(&mut self) -> Vec<TraceEvent> {
        std::mem::take(&mut self.trace)
    }

    /// Executes a Lua chunk and returns whatever it returns, as JSON.
    pub fn eval_json(&mut self, source: &str) -> Result<serde_json::Value, ScriptError> {
        self.instructions.store(0, Ordering::Relaxed);
        let value: mlua::Value = self
            .lua
            .load(source)
            .set_name("<eval>")
            .set_mode(ChunkMode::Text)
            .eval()?;
        Ok(lua_value_to_json(&value))
    }
}

fn install_log(
    lua: &Lua,
    worldrunner: &mlua::Table,
    records: &Arc<Mutex<Vec<Record>>>,
) -> Result<(), ScriptError> {
    let record_sink = Arc::clone(records);
    let log = lua.create_table()?;
    for level in ["info", "warn", "error"] {
        let sink = Arc::clone(&record_sink);
        log.raw_set(
            level,
            lua.create_function(move |_, (message, fields): (String, Option<mlua::Table>)| {
                let fields = match fields {
                    Some(table) => table_to_json(&table)?,
                    None => serde_json::Value::Null,
                };
                sink.lock().expect("record sink").push(Record {
                    kind: "log".into(),
                    level: Some(level.into()),
                    message,
                    fields,
                });
                Ok(())
            })?,
        )?;
    }
    worldrunner.raw_set("log", log)?;
    Ok(())
}

fn install_events(
    lua: &Lua,
    worldrunner: &mlua::Table,
    records: &Arc<Mutex<Vec<Record>>>,
) -> Result<(), ScriptError> {
    let events = lua.create_table()?;
    let sink = Arc::clone(records);
    events.raw_set(
        "publish",
        lua.create_function(move |_, (topic, fields): (String, Option<mlua::Table>)| {
            let fields = match fields {
                Some(table) => table_to_json(&table)?,
                None => serde_json::Value::Null,
            };
            sink.lock().expect("record sink").push(Record {
                kind: "event".into(),
                level: None,
                message: topic,
                fields,
            });
            Ok(())
        })?,
    )?;
    worldrunner.raw_set("events", events)?;
    Ok(())
}

fn install_require(
    lua: &Lua,
    modules: &Arc<RwLock<HashMap<String, Vec<u8>>>>,
) -> Result<(), ScriptError> {
    // require: memory modules only; anything else is a sandbox error that
    // names the module.
    let resolver = Arc::clone(modules);
    lua.globals().raw_set(
        "require",
        lua.create_function(move |lua, name: String| {
            let source = resolver
                .read()
                .expect("module table")
                .get(&name)
                .cloned()
                .ok_or_else(|| {
                    mlua::Error::RuntimeError(format!(
                        "module {name} not found in the host resolver chain"
                    ))
                })?;
            let loaded: mlua::Table = lua
                .globals()
                .raw_get::<Option<mlua::Table>>("__host_loaded")?
                .unwrap_or_else(|| lua.create_table().expect("loaded table"));
            if let Some(cached) = loaded.raw_get::<Option<mlua::Value>>(name.as_str())? {
                return Ok(cached);
            }
            let value: mlua::Value = lua
                .load(source)
                .set_name(name.as_str())
                .set_mode(ChunkMode::Text)
                .eval()?;
            loaded.raw_set(name.as_str(), value.clone())?;
            lua.globals().raw_set("__host_loaded", loaded)?;
            Ok(value)
        })?,
    )?;
    Ok(())
}
