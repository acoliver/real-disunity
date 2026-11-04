//! Brings the textures a player's managed code stored into the scene textures a frame draws with.
#[cfg(test)]
#[path = "player_ui_tests.rs"]
mod ui_tests;

use std::collections::{BTreeMap, BTreeSet};

use worldrunner_disunity_runtime::DisunityHost;
use worldrunner_render_plan::simulation::SpriteTextures;

use crate::{
    PlayError,
    textures::{BlockPath, player_scene_texture},
};

/// Player textures take renderer identities from here up; the managed engine allocates them.
pub const FIRST_PLAYER_TEXTURE: u64 = 1 << 40;

/// The window's device is created for the scene it starts with, which has no block-compressed
/// textures, so it cannot upload them; block formats are decoded here instead.
const BLOCK_PATH: BlockPath = BlockPath::Rgba8;

/// Which generation of each player texture the scene textures hold.
#[derive(Debug, Default)]
pub struct PlayerTextures {
    generations: BTreeMap<u64, u64>,
    reported: BTreeSet<u64>,
    /// The textures converted again since [`PlayerTextures::take_changed`] last ran.
    changed: BTreeSet<u64>,
}

impl PlayerTextures {
    pub(crate) fn generation(&self, identity: u64) -> u64 {
        self.generations.get(&identity).copied().unwrap_or(0)
    }

    /// The textures converted new or again since the last call.
    pub fn take_changed(&mut self) -> BTreeSet<u64> {
        std::mem::take(&mut self.changed)
    }

    /// Converts each player texture among the `shown` identities that is new or changed since the last
    /// call into `textures`. A texture that cannot be converted is named once on stderr and left
    /// out, so its quads stay unresolved.
    pub fn refresh(
        &mut self,
        host: &DisunityHost,
        shown: impl IntoIterator<Item = u64>,
        textures: &mut SpriteTextures,
    ) -> Result<(), PlayError> {
        let shown: BTreeSet<u64> = shown
            .into_iter()
            .filter(|identity| *identity >= FIRST_PLAYER_TEXTURE)
            .collect();
        for identity in shown {
            let known = self.generations.get(&identity).copied().unwrap_or(0);
            let Some(stored) = host.player_texture(identity, known)? else {
                continue;
            };
            self.generations.insert(identity, stored.generation);
            match player_scene_texture(&stored, BLOCK_PATH) {
                Ok(scene) => {
                    textures.textures.insert(identity, scene);
                    self.changed.insert(identity);
                }
                Err(error) => {
                    textures.textures.remove(&identity);
                    if self.reported.insert(identity) {
                        eprintln!("texture {identity}: {error}; its sprites are left out");
                    }
                }
            }
        }
        Ok(())
    }
}
