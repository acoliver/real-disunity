//! Plays the original Unity games from their scenes in a window.
//!
//! [`prepare`] fetches a game, plans its scene, decodes the scene's textures and builds its
//! scripts and the managed bridge. [`PlayDriver`] then runs the loaded world in the desktop
//! host: window keys reach the game's `Input`, each frame advances the world and renders its
//! presentation, and requested frames are captured.

mod driver;
mod keys;
mod prepare;
mod textures;

pub use driver::{Capture, PlayDriver};
pub use keys::key_code;
pub use prepare::{Prepared, prepare};
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
