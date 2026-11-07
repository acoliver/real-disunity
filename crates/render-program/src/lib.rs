//! A format-neutral runtime for shader programs: stage modules, resource and constant layouts,
//! render state, pipeline caching, bind groups, draws and dispatches, recorded into frames of a
//! caller's submission authority.
//!
//! Everything at the root of this crate is described in engine terms. The `unity` module,
//! behind the default-off `unity` feature, converts Unity's serialized programs into those
//! descriptions and no other module names a Unity type.

mod arena;
mod bind;
pub mod binding;
mod budget;
pub mod constants;
mod create;
mod dispatch;
mod draw;
pub mod error;
pub mod ids;
mod import;
pub mod layers;
pub mod layout;
pub mod material;
mod material_draw;
pub mod module;
pub mod offscreen;
mod packet;
mod pipeline_key;
mod program;
pub mod properties;
mod range;
mod readback;
mod reflect;
pub mod registry;
mod resources;
pub mod runtime;
pub mod sampler;
mod schema;
pub mod state;
pub mod targets;
#[cfg(feature = "unity")]
pub mod unity;
pub mod variant;

/// The native guide's code blocks, compiled as doctests so the guide cannot drift from the API.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/render-program-native.md")]
pub struct NativeGuide;
