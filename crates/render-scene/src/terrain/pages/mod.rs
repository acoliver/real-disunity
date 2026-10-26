//! Adaptive material-page preparation inside the scene terrain renderer.
//!
//! A page addresses a normalized rectangular heightfield, not a fixed number
//! of geometry intervals. Color is linear RGBA8; tangent normals and perceptual
//! roughness share a second RGBA8 product. Generated mips beyond the configured
//! filter-safe limit are not eligible for sampling.

#[cfg(test)]
mod blocking_tests;
pub mod composition;
pub mod gpu;
#[cfg(test)]
mod gpu_tests;
pub mod layered;
pub mod layered_gpu;
#[cfg(test)]
mod lookup_work_tests;
pub mod mip_gpu;
#[cfg(test)]
mod mip_gpu_tests;
pub mod mips;
#[cfg(test)]
mod normal_tie_tests;
#[cfg(test)]
mod readback_tests;
pub mod residency;
#[cfg(test)]
mod residency_budget_tests;
#[cfg(test)]
mod residency_edge_tests;
#[cfg(test)]
mod residency_gpu_tests;
#[cfg(test)]
mod residency_tests;
pub mod scene;
pub mod selection;
#[cfg(test)]
mod slot_gpu_tests;
#[cfg(test)]
mod submission_gpu_tests;
pub mod view;
#[cfg(test)]
mod view_tests;

#[cfg(test)]
mod edge_tests;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod sampling_tests;

#[cfg(test)]
mod scene_tests;
