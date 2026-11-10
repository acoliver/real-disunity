//! Registering the stored textures a snapshot names: reading what the engine stored a new
//! generation of, uploading it, and keeping the adapter's record of it in step with what the
//! state holds.

use std::collections::BTreeSet;

use worldrunner_disunity_runtime::{PlayerTexture, TextureSampler};
use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::runtime::ProgramRuntime;
use worldrunner_render_program::unity::offscreen::OffscreenState;

use super::super::assets::{Assets, sampler_of_stored, texture_of};
use super::super::frame::Report;
use super::super::ids;
use super::{OffscreenAdapter, Stored, is_over_budget};

/// What uploading a texture the engine stored came to.
#[derive(Clone, Copy)]
enum TextureUpload {
    /// It is in the state.
    Held,
    /// It cannot be uploaded for a reason that is its own or the device's, which stays so until
    /// the engine stores a new generation of it.
    Unusable,
    /// The storage budget refused it, which a later snapshot may find otherwise.
    OverBudget,
}

impl OffscreenAdapter {
    /// Reads, uploads and records each texture of `identities` the engine has a new generation
    /// of, or a new sampler for.
    pub(super) fn register_textures(
        &mut self,
        identities: &BTreeSet<u64>,
        assets: &dyn Assets,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        notes: &mut Report,
    ) {
        for &identity in identities {
            let Some(id) = ids::stored_texture(identity) else {
                notes.count("texture:IdentityTooLarge");
                continue;
            };
            let held = self.textures.get(&identity).copied();
            let stored_sampler = assets.sampler(identity).unwrap_or_else(|_| {
                notes.count("texture:SamplerRead");
                None
            });
            // A sampler that changed alone replaces the texture, whose bytes are read again.
            let sampler_changed = stored_sampler.as_ref().is_some_and(|found| {
                held.is_none_or(|held| held.sampler_generation != found.generation)
            });
            let known = held
                .filter(|_| !sampler_changed)
                .map_or(0, |held| held.generation);
            let stored = match assets.texture(identity, known) {
                Ok(Some(stored)) => stored,
                // None is a texture the engine does not hold, or one it holds unchanged.
                Ok(None) => {
                    if held.is_none() {
                        notes.count("texture:NotStored");
                    }
                    continue;
                }
                Err(_) => {
                    notes.count("texture:Read");
                    continue;
                }
            };
            let upload = self.upload_texture(
                id,
                (&stored, stored_sampler.as_ref()),
                (state, runtime),
                notes,
            );
            let generations = (
                stored.generation,
                stored_sampler.map_or(0, |found| found.generation),
            );
            self.note_texture((identity, id), generations, upload, (state, runtime));
        }
    }

    /// Uploads a texture the engine stored a new generation of, in place of the one held.
    fn upload_texture(
        &mut self,
        id: TextureId,
        (stored, sampler): (&PlayerTexture, Option<&TextureSampler>),
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
        notes: &mut Report,
    ) -> TextureUpload {
        let blocks = self.block_sources
            && runtime
                .device()
                .features()
                .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        let texture = match texture_of(id, stored, sampler.map(sampler_of_stored), blocks) {
            Ok(texture) => texture,
            Err(error) => {
                notes.fail("texture", &error);
                notes.count(&format!(
                    "textureformat:{} {}x{}, {} bytes",
                    crate::textures::format_name(stored.format),
                    stored.width,
                    stored.height,
                    stored.bytes.len()
                ));
                return TextureUpload::Unusable;
            }
        };
        let uploaded = self.with_room(state, runtime, notes, |state, runtime| {
            state.set_texture(runtime, &texture)
        });
        match uploaded {
            Ok(()) => {
                let census = format!(
                    "stored texture {} ({}) uploaded as GraphicsFormat {}",
                    crate::textures::format_name(stored.format),
                    if stored.linear { "linear" } else { "sRGB" },
                    texture.graphics_format
                );
                *notes.census.entry(census).or_default() += 1;
                TextureUpload::Held
            }
            Err(error) => {
                notes.fail("texture", &error);
                // Only a refusal for room is known to pass. Any other is read as before this
                // distinction was made: once for each generation.
                if is_over_budget(&error) {
                    TextureUpload::OverBudget
                } else {
                    TextureUpload::Unusable
                }
            }
        }
    }

    /// Keeps what an upload came to. The state holds the texture exactly when the adapter's entry
    /// is usable: any other outcome gives back the storage of the texture that was held, so a
    /// replacement that failed leaves no old texels behind.
    fn note_texture(
        &mut self,
        (identity, id): (u64, TextureId),
        (generation, sampler_generation): (u64, u64),
        upload: TextureUpload,
        (state, runtime): (&mut OffscreenState, &mut ProgramRuntime),
    ) {
        let usable = match upload {
            TextureUpload::Held => true,
            TextureUpload::Unusable => {
                state.release_source_texture(runtime, id);
                false
            }
            TextureUpload::OverBudget => {
                // Without the generation, the engine gives the whole texture again next time.
                state.release_source_texture(runtime, id);
                self.textures.remove(&identity);
                return;
            }
        };
        self.textures.insert(
            identity,
            Stored {
                generation,
                sampler_generation,
                usable,
                empty: false,
                last_named: self.epoch,
            },
        );
    }
}
