//! A live fixture driver over the canonical native world and resident renderer.

mod admission;
mod desktop;
mod dotnet;
mod extraction;
mod graphics;
mod installation;
mod interactive;
mod interactive_log;
mod live_input;
mod managed_build;
mod native_image;
mod playback;
mod resources;
mod session;
mod strict;
mod strict_events;
mod strict_ingress;
mod teardown;
mod tiles;
mod trace;
mod ui;
mod worker;

pub use installation::Installation;
pub use managed_build::compile_game;
pub use session::Session;
pub use worker::{run_request, run_worker};

#[cfg(test)]
mod extraction_tests;

#[cfg(test)]
mod one_host;

#[cfg(test)]
mod test_support;
