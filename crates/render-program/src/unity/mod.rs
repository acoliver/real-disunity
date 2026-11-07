//! The Unity adapter: converts Unity's serialized shader programs, parameters and pass state
//! into the neutral descriptions the rest of the crate executes.
//!
//! This is the only module that names `worldrunner_unity_serialized`. Its submodules keep the
//! shapes the crate had before it was format-neutral (Unity binding tables, parameter
//! records, keyword indices, `ShaderState`), each wrapping or producing neutral values, so a
//! caller that holds Unity records routes through `unity::module`, `unity::layout`,
//! `unity::constants`, `unity::state`, `unity::variant` and `unity::runtime`.
//!
//! Unity's policies stay here: an enabled keyword no variant differs by is ignored, and a
//! vector or matrix is written one 16-byte register at a time.

pub mod constants;
mod declare;
pub mod layout;
pub mod module;
pub mod runtime;
pub mod state;
mod targets;
pub mod variant;
