//! ABI session ownership and transport over the typed native simulation world.

pub mod abi;
#[cfg(test)]
mod allocation;
pub mod assets;
pub mod buffers;
#[cfg(test)]
mod callback_tests;
pub mod callbacks;
pub mod camera_data;
pub mod camera_layers;
pub mod cloning;
#[cfg(test)]
mod cloning_tests;
mod commands;
pub mod errors;
pub mod hierarchy;
pub mod input;
pub mod object_api;
pub mod physics;
#[cfg(test)]
mod physics_material_tests;
pub mod physics_materials;
#[cfg(test)]
mod physics_tests;
pub mod presentation;
pub mod scheduling;
pub mod sprites;
mod store;
#[cfg(test)]
mod time_input_tests;
pub mod ui;
mod views;

pub use abi::{DISUNITY_ABI_VERSION, DisunityResult, result_code};
pub use store::{store_statistics, with_native_world};
pub use views::{DisunityObject, DisunityWorld, export_object_states, with_world};

use commands::submit_commands;
use store::{create_object, create_world, destroy_object, destroy_world, lock_store, record_error};

/// Maximum commands accepted by a single foreign submission.
pub const MAX_COMMANDS_PER_SUBMIT: u32 = 1 << 20;

pub mod command_kind {
    pub const SET_POSITION: u32 = 0;
    pub const SET_ROTATION_Y: u32 = 1;
    pub const SET_MATERIAL_SLOT: u32 = 2;
}

/// A blittable frame intent: `object` is the target handle, `kind` selects the
/// operation, and `x`/`y`/`z` carry the operands. `SET_POSITION` uses all
/// three; `SET_ROTATION_Y` and `SET_MATERIAL_SLOT` use `x` (slots stay exact
/// because they are small integers).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityCommand {
    pub object: u64,
    pub kind: u32,
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

/// A blittable snapshot of one live object, for bulk extraction by a host
/// renderer. Batched like the command buffer: one call, no per-object FFI.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisunityObjectState {
    pub object: u64,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub rotation_y_radians: f32,
    pub material_slot: u32,
}

#[cfg(test)]
mod generated_layouts {
    include!(concat!(env!("OUT_DIR"), "/NativeLayouts.g.rs"));
}
