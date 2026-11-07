use super::{AllocationSet, RendererConfiguration};
pub(super) fn allocations(settings: &RendererConfiguration, set: &mut AllocationSet) {
    if settings.ui_depth
        || !settings.water.is_empty()
        || settings.ssr.is_some_and(|ssr| ssr.enabled)
    {
        let [w, h] = settings.frame.size;
        set.texture(
            "snapshot color",
            [w, h, 1],
            wgpu::TextureFormat::Rgba16Float,
            [1, 1],
            3,
        );
        set.texture(
            "snapshot depths",
            [w, h, 1],
            wgpu::TextureFormat::R32Float,
            [1, 1],
            6,
        );
        set.buffer("snapshot uniforms", 304, 3);
        set.buffer("snapshot staging", 192, 3);
        if !settings.water.is_empty() {
            set.texture(
                "water receivers",
                [w, h, 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, settings.frame.samples],
                9,
            );
            set.texture(
                "empty water environment",
                [1, 1, 6],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                1,
            );
            set.texture(
                "empty water BRDF",
                [1, 1, 1],
                wgpu::TextureFormat::Rgba16Float,
                [1, 1],
                1,
            );
        }
        for water in &settings.water {
            crate::water::allocations(water, set);
        }
    }
    if settings.ui_depth {
        set.buffer("annotation depth reconstruction", 80, 3);
    }
}
