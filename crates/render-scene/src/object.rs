//! Object materials on the program runtime, slice 2 of issue 450.
//!
//! Every object pass draws lit objects, foliage and impostors through [`program`] and
//! [`prepared`]: the forward passes (plain, velocity, SSAO, reflection and the shadow receivers
//! of each), the metadata prepass, the capture term replays and the shadow casters.
//! Celestial meshes (`sky::meshes`) still draw with fixed pipelines of their own.

mod casting;
pub(crate) mod layout;
pub(crate) mod prepared;
pub(crate) mod program;
pub(crate) mod skin_source;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
pub(crate) mod cost_tests;
#[cfg(test)]
mod program_tests;
