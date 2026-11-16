use worldrunner_render_gpu::census::CensusSnapshot;
use worldrunner_render_scene::draw::PreparedScene;
use worldrunner_render_scene::texture::{TextureTierPolicy, TierChoice};

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct TextureTiers {
    pub policy: Option<TextureTierPolicy>,
    pub chosen: Option<TierChoice>,
}

impl TextureTiers {
    pub fn apply(
        &self,
        context: &super::GpuContext,
        prepared: &mut PreparedScene,
    ) -> Option<TierChoice> {
        self.policy?;
        let memory = context
            .adapter_identity()
            .memory
            .map(|memory| memory.device_local);
        let resident = context.submission().census().snapshot();
        self.prepare(prepared, memory, resident)
    }

    fn prepare(
        &self,
        prepared: &mut PreparedScene,
        memory: Option<u64>,
        resident: CensusSnapshot,
    ) -> Option<TierChoice> {
        let policy = self.policy?;
        let reducible = prepared.scene().resources().reducible_textures().to_vec();
        let fixed = prepared.texture_bytes_where(|handle| !reducible.contains(&handle));
        let other = resident.total().saturating_add(fixed);
        Some(
            prepared.apply_texture_tier(Some(policy), memory, other, |handle| {
                reducible.contains(&handle)
            }),
        )
    }
}

#[cfg(test)]
mod tests;
