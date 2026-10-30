//! worldrunner-script: the engine's sandboxed Lua 5.4 script host.
//!
//! The host owns the runtime; scripts receive only `worldrunner.*` handles.
//! There is no filesystem access, no wall clock, no unsafe stdlib, and every
//! hook fire runs under an instruction budget.

pub mod api;
pub mod config;
mod coroutines;
mod division;
pub mod error;
pub mod host;
mod legacy;
mod load;
pub mod modules;
mod options;
pub mod original;
pub mod patches;
pub mod persist;
mod resume;
mod sandbox;
pub mod threads;
pub mod world;

pub use api::ScriptApi;
pub use config::{DivisionSemantics, NameCollisions, RefusedChunks, ScriptConfig};
pub use error::ScriptError;
pub use host::{API_VERSION, DEFAULT_MEMORY_LIMIT, HookContext, HookResult, Record, ScriptHost};
pub use lua_resume;
pub use modules::{AssetResolver, AssetsApi, WorldApi};
pub use original::GameScripts;
pub use patches::{Edit, Overlay, Patch, PatchError, PatchSet, Patched, sha256_hex};
pub use persist::{LuaSnapshot, Saved, SavedTable, SavedThread, SavedWake};
pub use threads::{
    ClockNative, Fraction, NegativeSleep, NewThread, Order, ResumeValue, SchedulerNames,
    SchedulerPolicy, ThreadFailure, WaitKind, ZeroSleep,
};
pub use world::{EntityHandle, WorldView};
