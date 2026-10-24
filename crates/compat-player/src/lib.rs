//! A live fixture driver over the canonical native world and resident renderer.

mod admission;
mod assembly;
mod assembly_probe;
pub mod assembly_qualification;
mod desktop;
mod extraction;
mod graphics;
pub mod image_lease;
mod interactive;
mod interactive_evidence;
mod live_input;
mod native_image;
mod playback;
mod process_image;
mod provider;
pub mod providers;
pub mod resource_providers;
mod resources;
mod session;
pub mod source_proof;
mod strict;
mod strict_events;
mod strict_ingress;
mod teardown;
mod tiles;
mod trace;
pub mod trusted;
mod ui;
mod worker;

pub use session::Session;
pub use worker::{run_request, run_worker};

#[cfg(test)]
mod extraction_tests;

#[cfg(test)]
#[path = "../../disunity-runtime/tests/support/handshake.rs"]
mod managed_test_artifacts;
