//! The fixed object forward path as it was before objects drew through the program runtime.
//!
//! This is a frozen copy, kept as test-only code until the fixed object pipelines are removed
//! from the scene. It is the reference of the byte-equality captures in `object::capture_tests`:
//! those draw the same objects through the scene and through this copy and compare every
//! attachment byte for byte.
//!
//! The pipeline builders, layouts, source composition and command sequence below are the ones
//! `renderer.rs`, `renderer/objects.rs`, `upload.rs`, `pipeline.rs` and `draw/lod_material.rs`
//! held, with these differences only:
//!
//! - The shader sources are composed from the snapshots in `object/fixtures`, and the guard
//!   `the_live_object_wgsl_equals_the_frozen_snapshot` fails when the live WGSL drifts from them.
//! - The options, layouts and targets a pipeline set is built from are read from the
//!   [`ScenePipelines`](super::ScenePipelines) selected for the frame, instead of passed down
//!   from their constructor.
//! - Recording goes through [`Commands`], which a render pass and a counter implement, so a test
//!   can count the commands the fixed path issues. The ranges it records are read from the scene
//!   with `SceneGpu::legacy_objects`. Compacted views are not part of the freeze: they are
//!   pinned against the ranged path by `draw::tests::compaction`.
//!
//! Do not edit the behaviour of this file. A change to object output is a change to what the
//! captures mean and is its own proposal.

mod commands;
mod depth;
mod layouts;
mod pipelines;
mod record;
mod source;

pub(crate) use commands::{Commands, Counted};
pub(crate) use depth::Depth;
pub(crate) use pipelines::{Build, Pipelines};
pub(crate) use record::{Draw, Level, Objects, Recorded, Slot, record_objects};
pub(crate) use source::{Recipe, compose, gathered};
