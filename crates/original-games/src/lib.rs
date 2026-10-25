//! Pinned, read-only integration sources shared by Rust, managed tests and tools.
use std::path::{Path, PathBuf};

mod cache;
#[cfg(test)]
mod cache_tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Game {
    Snake,
    Breakout,
    Pong,
}
impl Game {
    pub const ALL: [Self; 3] = [Self::Snake, Self::Breakout, Self::Pong];

    pub fn name(self) -> &'static str {
        match self {
            Self::Snake => "snake",
            Self::Breakout => "breakout",
            Self::Pong => "pong",
        }
    }
    pub fn rev(self) -> &'static str {
        match self {
            Self::Snake => "0ab43c53e834db714439be71c4151d953d241847",
            Self::Breakout => "2b960a3589f30b6e0025f2509ef00bf9155d6f45",
            Self::Pong => "e2d3368728e236b0ab5a04afb142a3ae49260dd1",
        }
    }
    pub fn url(self) -> &'static str {
        match self {
            Self::Snake => "https://github.com/eguldali/Optimized-Snake-Game.git",
            Self::Breakout => "https://github.com/admtrv/Breakout.git",
            Self::Pong => "https://github.com/mcrvaz/pong-tdd.git",
        }
    }
}
impl std::str::FromStr for Game {
    type Err = Error;
    fn from_str(name: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|game| game.name() == name)
            .ok_or_else(|| Error::UnknownGame(name.into()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("unknown original game: {0}")]
    UnknownGame(String),
    #[error("workspace root not found from {0}")]
    Workspace(PathBuf),
    #[error("git {operation} failed in {path}: {message}")]
    Git {
        operation: String,
        path: PathBuf,
        message: String,
    },
    #[error("original checkout {path} has HEAD {actual}, expected {expected}")]
    Revision {
        path: PathBuf,
        actual: String,
        expected: String,
    },
    #[error("original checkout has local changes: {0}")]
    Modified(PathBuf),
}

pub fn workspace_root() -> Result<PathBuf, Error> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest
        .ancestors()
        .find(|path| {
            std::fs::read_to_string(path.join("Cargo.toml"))
                .is_ok_and(|text| text.lines().any(|line| line.trim() == "[workspace]"))
        })
        .map(Path::to_path_buf)
        .ok_or_else(|| Error::Workspace(manifest.into()))
}

pub fn checkout(game: Game) -> Result<PathBuf, Error> {
    checkout_in(&workspace_root()?, game)
}

/// Fetches into an explicitly selected workspace's target directory.
pub fn checkout_in(workspace: &Path, game: Game) -> Result<PathBuf, Error> {
    cache::checkout(workspace, game)
}

fn checkout_path(workspace: &Path, game: Game) -> PathBuf {
    workspace
        .join("target/original-games")
        .join(format!("{}-{}", game.name(), game.rev()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pins_are_exact_and_paths_do_not_depend_on_the_build_target() {
        for game in Game::ALL {
            assert_eq!(game.rev().len(), 40);
            assert!(game.rev().bytes().all(|byte| byte.is_ascii_hexdigit()));
            assert_eq!(game.name().parse::<Game>().unwrap(), game);
            assert!(game.url().starts_with("https://github.com/"));
            assert_eq!(
                checkout_path(Path::new("/workspace"), game),
                Path::new("/workspace/target/original-games").join(format!(
                    "{}-{}",
                    game.name(),
                    game.rev()
                ))
            );
        }
        assert!("../snake".parse::<Game>().is_err());
        assert!(
            workspace_root()
                .unwrap()
                .join("crates/original-games/Cargo.toml")
                .is_file()
        );
    }
}
