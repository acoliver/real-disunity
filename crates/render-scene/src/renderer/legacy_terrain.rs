//! The fixed terrain path as it was before terrain drew through the program runtime.
//!
//! This is a frozen copy, kept as test-only code until the fixed pipelines are removed. It is
//! the reference of the byte-equality captures in `terrain::capture_tests`: those draw the same
//! terrain through the scene and through this copy and compare every attachment byte for byte.
//! The pipeline builders, layouts, bind group builders and the command sequence below are the
//! ones `renderer.rs`, `renderer/pages.rs`, `upload.rs`, `draw/resources.rs`, `draw.rs` and
//! `draw/streaming.rs` held, with these differences only:
//!
//! - The sources and options a pipeline set is built from are read from the [`ScenePipelines`]
//!   that were selected for the frame, instead of passed down from their constructor.
//! - Recording goes through [`Commands`], which a render pass and a counter implement, so a test
//!   can count the commands the fixed path issues.
//! - The page files are read from the frozen snapshots in `terrain/fixtures`.
//! - Main's depth reuse is part of the freeze: the pipelines carry the depth and stencil state of
//!   the [`Depth`] mode the selected pipelines were built for, copied from `renderer/depth.rs` as
//!   it was when the freeze was taken. The copy is the reference for the state the runtime path
//!   declares, so a change to the live state shows as a capture difference.
//!
//! Do not edit the behaviour of this file. A change to terrain output is a change to what the
//! captures mean and is its own proposal.

mod commands;
mod depth;
mod draw;
mod groups;
mod layouts;
mod pipelines;

pub(crate) use commands::{Commands, Counted};
pub(crate) use depth::Depth;
pub(crate) use draw::{Chunk, Geometry, Surface, draw_terrain_color};
pub(crate) use groups::{Resources, terrain_bind_group};
pub(crate) use pipelines::{Build, Pipelines};
