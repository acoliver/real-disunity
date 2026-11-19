//! A game adds its own passes, with its own targets, to the retained renderer through the
//! inventory on the renderer configuration. The frames are real: a configured legacy renderer
//! over a synthetic unlit scene on lavapipe, with the game's passes on compiled program draws.
#[path = "game_passes/accounting.rs"]
mod accounting;
#[path = "game_passes/common.rs"]
mod common;
#[path = "game_passes/composition.rs"]
mod composition;
#[path = "game_passes/fixture.rs"]
mod fixture;
#[path = "game_passes/layers.rs"]
mod layers;
#[path = "game_passes/persistence.rs"]
mod persistence;
#[path = "game_passes/prepared.rs"]
mod prepared;
#[path = "game_passes/refusals.rs"]
mod refusals;
#[path = "game_passes/replacement.rs"]
mod replacement;
#[path = "game_passes/shadowed.rs"]
mod shadowed;
#[path = "game_passes/stores.rs"]
mod stores;
#[path = "game_passes/strip.rs"]
mod strip;
#[path = "game_passes/terrain.rs"]
mod terrain;
