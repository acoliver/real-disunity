//! Plays the original Unity games from their scenes in a window.
//!
//! [`prepare`] fetches a game, plans its scene, decodes the scene's textures and builds its
//! scripts and the managed bridge. [`play_game_window`] then runs the loaded world in the
//! retained host's window: window keys reach the game's `Input`, each frame advances the world and
//! shows its presentation, and requested frames are captured.
//!
//! [`prepare_player`] plans a built Unity player's scene. [`play_player_window`] plays it in the
//! retained host's window, [`play_player_windowless`] plays it in the same host with no window or
//! display and returns the frames asked for, and [`run_player`] starts it with no viewer at all,
//! naming the first step that fails.

mod bc7;
mod bc7_tables;
#[cfg(test)]
mod bc7_tests;
mod block_texture;
mod camera_start;
mod counts_line;
mod driver;
mod global_listing;
mod gpu_device;
#[cfg(test)]
mod gpu_flush_tests;
mod gpu_player;
mod keys;
#[cfg(test)]
mod managed_array_tests;
pub mod offscreen;
mod plan_differences;
mod player;
mod player_meshes;
mod player_textures;
mod player_ui;
mod player_ui_materials;
mod prepare;
mod retained_driver;
mod scene_census;
mod scene_light;
mod scene_tables;
mod scene_transform;
pub mod storage_budget;
mod target_read;
mod textures;
#[cfg(test)]
mod textures_tests;
mod unresolved;
mod window_script;

pub use driver::{Capture, play_game_window};
pub use gpu_device::compute_census;
pub use gpu_player::{GpuLaunch, GpuWorkReport, PlayerGpu};
pub use keys::key_code;
pub use offscreen::{StandInLook, SuppliedGlobal};
pub use player::{PlayerLaunch, PreparedPlayer, prepare_player, run_player};
pub use player_meshes::{
    MeshConversionError, PlayerMeshes, PlayerSceneMesh, SlotRun, player_scene_mesh,
};
pub use prepare::{Prepared, prepare};
pub use retained_driver::{PlayerWindow, ScheduledKey, play_player_window, play_player_windowless};
pub use target_read::TargetRead;
pub use worldrunner_original_games::Game;

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum PlayError {
    #[error(transparent)]
    Games(#[from] worldrunner_original_games::Error),
    #[error(transparent)]
    Import(#[from] worldrunner_unity_import::ImportError),
    #[error(transparent)]
    Plan(#[from] worldrunner_unity_import::PlanError),
    #[error(transparent)]
    Texture(#[from] worldrunner_unity_import::TextureError),
    #[error(transparent)]
    Player(#[from] worldrunner_unity_import::PlayerError),
    #[error(transparent)]
    Layouts(#[from] worldrunner_unity_import::script_layouts::LayoutError),
    #[error(transparent)]
    Host(#[from] worldrunner_disunity_runtime::HostError),
    #[error("{0}: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("{0}")]
    Invalid(String),
}

/// The scene each game starts in.
#[must_use]
pub fn scene(game: Game) -> &'static str {
    match game {
        Game::Snake => "Assets/Scenes/GameScene.unity",
        Game::Breakout => "Assets/Scenes/SampleScene.unity",
        Game::Pong => "Assets/Scenes/Main.unity",
    }
}
