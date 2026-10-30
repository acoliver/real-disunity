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
mod import;
mod legacy;
mod load;
pub mod modules;
mod options;
pub mod original;
mod overrun;
pub mod patches;
pub mod persist;
mod resume;
mod sandbox;
pub mod threads;
pub mod world;

pub use api::ScriptApi;
pub use config::{
    DivisionSemantics, NameCollisions, OnOverrun, RefusedChunks, ResumeBudget, ScriptConfig,
};
pub use error::ScriptError;
pub use host::{API_VERSION, DEFAULT_MEMORY_LIMIT, HookContext, HookResult, Record, ScriptHost};
pub use import::PermanentSpec;
pub use lua_resume;
pub use modules::{AssetResolver, AssetsApi, WorldApi};
pub use original::GameScripts;
pub use patches::{Edit, Overlay, Patch, PatchError, PatchSet, Patched, sha256_hex};
pub use persist::{
    LuaSnapshot, SaveOptions, Saved, SavedTable, SavedThread, SavedWake, ThreadSchedule,
};
pub use threads::{
    AdoptWait, Adoption, ClockNative, FlagNatives, FlaggedSpawn, Fraction, NegativeSleep,
    NewThread, Order, PersistedThread, ResumeValue, SchedulerNames, SchedulerPolicy, ThreadFailure,
    ThreadInfo, ThreadWait, WaitKind, WakeNative, ZeroSleep,
};
pub use world::{EntityHandle, WorldView};
