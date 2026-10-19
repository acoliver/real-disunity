//! worldrunner-script: the engine's sandboxed Lua 5.4 script host.
//!
//! The host owns the runtime; scripts receive only `worldrunner.*` handles.
//! There is no filesystem access, no wall clock, no unsafe stdlib, and every
//! hook fire runs under an instruction budget.

pub mod api;
pub mod config;
pub mod error;
pub mod host;
pub mod modules;
pub mod world;

pub use api::ScriptApi;
pub use config::ScriptConfig;
pub use error::ScriptError;
pub use host::{API_VERSION, HookContext, HookResult, Record, ScriptHost};
pub use modules::{AssetResolver, AssetsApi, WorldApi};
pub use world::{EntityHandle, WorldView};
