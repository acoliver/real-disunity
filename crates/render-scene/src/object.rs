//! Object materials on the program runtime, slice 2 of issue 450.
//!
//! The forward passes (plain, velocity, SSAO, reflection and the shadow receivers of each) draw
//! lit objects, foliage and impostors through [`program`] and [`prepared`]. The fixed path is
//! frozen in [`crate::renderer::legacy_objects`] and pinned by byte-equality captures; the
//! shadow casters, the metadata prepass, the capture terms and the celestial meshes still draw
//! with the fixed pipelines.

pub(crate) mod prepared;
pub(crate) mod program;

#[cfg(test)]
mod capture_tests;
#[cfg(test)]
pub(crate) mod cost_tests;
#[cfg(test)]
mod program_tests;
