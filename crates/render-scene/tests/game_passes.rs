//! A game adds its own passes, with its own targets, to the retained renderer through the
//! inventory on the renderer configuration. The frames are real: a configured legacy renderer
//! over a synthetic unlit scene on lavapipe, with the game's passes on compiled program draws.
#[path = "game_passes/accounting.rs"]
mod accounting;
#[path = "game_passes/before_post.rs"]
mod before_post;
#[path = "game_passes/common.rs"]
mod common;
#[path = "game_passes/composition.rs"]
mod composition;
#[path = "game_passes/compute.rs"]
mod compute;
#[path = "game_passes/declaring.rs"]
mod declaring;
#[path = "game_passes/environment.rs"]
mod environment;
#[path = "game_passes/fixture.rs"]
mod fixture;
#[path = "game_passes/layers.rs"]
mod layers;
#[path = "game_passes/lent.rs"]
mod lent;
#[path = "game_passes/lent_capture.rs"]
mod lent_capture;
#[path = "game_passes/lent_chunk.rs"]
mod lent_chunk;
#[path = "game_passes/lent_engine.rs"]
mod lent_engine;
#[path = "game_passes/lent_prefix.rs"]
mod lent_prefix;
#[path = "game_passes/lent_terrain.rs"]
mod lent_terrain;
#[path = "game_passes/lent_terrain_budget.rs"]
mod lent_terrain_budget;
#[path = "game_passes/lent_terrain_game.rs"]
mod lent_terrain_game;
#[path = "game_passes/level_import.rs"]
mod level_import;
#[path = "game_passes/lighting.rs"]
mod lighting;
#[path = "game_passes/mips.rs"]
mod mips;
#[path = "game_passes/persistence.rs"]
mod persistence;
#[path = "game_passes/post_water_depth.rs"]
mod post_water_depth;
#[path = "game_passes/post_water_game.rs"]
mod post_water_game;
#[path = "game_passes/prefix.rs"]
mod prefix;
#[path = "game_passes/prefix_idle.rs"]
mod prefix_idle;
#[path = "game_passes/prefix_loads.rs"]
mod prefix_loads;
#[path = "game_passes/prefix_state.rs"]
mod prefix_state;
#[path = "game_passes/prepared.rs"]
mod prepared;
#[path = "game_passes/pyramid_model.rs"]
mod pyramid_model;
#[path = "game_passes/refusals.rs"]
mod refusals;
#[path = "game_passes/render_mips.rs"]
mod render_mips;
#[path = "game_passes/replacement.rs"]
mod replacement;
#[path = "game_passes/resource_mips.rs"]
mod resource_mips;
#[path = "game_passes/resources.rs"]
mod resources;
#[path = "game_passes/scene_arrays.rs"]
mod scene_arrays;
#[path = "game_passes/shadow_cascades.rs"]
mod shadow_cascades;
#[path = "game_passes/shadowed.rs"]
mod shadowed;
#[path = "game_passes/snapshot_capture.rs"]
mod snapshot_capture;
#[path = "game_passes/snapshot_game.rs"]
mod snapshot_game;
#[path = "game_passes/snapshot_gaps.rs"]
mod snapshot_gaps;
#[path = "game_passes/snapshot_water.rs"]
mod snapshot_water;
#[path = "game_passes/snapshots.rs"]
mod snapshots;
#[path = "game_passes/stores.rs"]
mod stores;
#[path = "game_passes/strip.rs"]
mod strip;
#[path = "game_passes/terrain.rs"]
mod terrain;
#[path = "game_passes/terrain_lender.rs"]
mod terrain_lender;
#[path = "game_passes/transfer_readback.rs"]
mod transfer_readback;
