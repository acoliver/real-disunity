//! Object materials on the program runtime, slice 2 of issue 450.
//!
//! Every object pass draws lit objects, foliage and impostors through [`program`] and
//! [`prepared`]: the forward passes (plain, velocity, SSAO, reflection and the shadow receivers
//! of each), the metadata prepass, the capture term replays and the shadow casters. The fixed
//! path is frozen in [`crate::renderer::legacy_objects`] and pinned by byte-equality captures.
//! Celestial meshes (`sky::meshes`) still draw with fixed pipelines of their own.

pub(crate) mod prepared;
pub(crate) mod program;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
pub(crate) mod cost_tests;
#[cfg(test)]
mod program_tests;
