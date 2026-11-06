//! The asset reads a flush handler shares with [`crate::DisunityHost`]: mesh channels, player
//! textures and their samplers. The world is borrowed by the managed call a flush runs inside, so
//! a handler that meets a mesh or texture for the first time during a flush reads it here.
use crate::{GpuFlush, HostError, MeshChannels, PlayerTexture, TextureSampler};

impl GpuFlush<'_> {
    /// The channels stored under mesh `identity`; see [`crate::DisunityHost::mesh_channels`].
    pub fn mesh_channels(&self, identity: u64) -> Result<Option<MeshChannels>, HostError> {
        self.mesh_channels_since(identity, 0)
    }

    /// See [`crate::DisunityHost::mesh_channels_since`].
    pub fn mesh_channels_since(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<MeshChannels>, HostError> {
        crate::mesh_channels::read_channels(
            self.world,
            self.calls.mesh_channels,
            identity,
            known_generation,
            |result| self.check(result),
        )
    }

    /// The texture stored under renderer `identity`; see [`crate::DisunityHost::player_texture`].
    pub fn player_texture(
        &self,
        identity: u64,
        known_generation: u64,
    ) -> Result<Option<PlayerTexture>, HostError> {
        crate::textures::read_texture(
            self.world,
            self.calls.texture,
            identity,
            known_generation,
            |result| self.check(result),
        )
    }

    /// The sampler of the texture stored under renderer `identity`; see
    /// [`crate::DisunityHost::texture_sampler`].
    pub fn texture_sampler(&self, identity: u64) -> Result<Option<TextureSampler>, HostError> {
        crate::textures::read_sampler(self.world, self.calls.sampler, identity, |result| {
            self.check(result)
        })
    }
}
