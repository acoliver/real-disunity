//! Local facade consumer. GPU execution requires scene + gfx + world.
//! Native capture stamps, timestamped input and live-session readiness remain upstream gaps.

#[cfg(feature = "scene")]
#[path = "migration_consumer/camera.rs"]
mod camera;
#[cfg(feature = "world")]
#[path = "migration_consumer/generation.rs"]
mod generation;
#[cfg(all(feature = "scene", feature = "gfx", feature = "world"))]
#[path = "migration_consumer/gpu_gate.rs"]
mod gpu_gate;
#[cfg(feature = "scene")]
#[path = "migration_consumer/policy.rs"]
mod policy;
