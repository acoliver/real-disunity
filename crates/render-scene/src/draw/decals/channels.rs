use super::{Decals, DrawError, RenderScene, empty_texture};
use worldrunner_render_plan::{SceneMaterial, TextureHandle};

#[derive(Clone, Copy)]
pub(super) enum Channel {
    Albedo,
    Normal,
    Surface,
}
impl Channel {
    pub fn handle(self, material: &SceneMaterial) -> Option<TextureHandle> {
        match self {
            Self::Albedo => Some(material.albedo()),
            Self::Normal => material.normal(),
            Self::Surface => material.roughness_metallic(),
        }
    }
    pub fn materials(self, scene: &RenderScene, materials: &[u32]) -> Vec<u32> {
        materials
            .iter()
            .copied()
            .filter(|&id| {
                self.handle(&scene.resources().materials()[id as usize])
                    .is_some()
            })
            .collect()
    }
    pub fn validate(self, scene: &RenderScene, materials: &[u32]) -> Result<(), DrawError> {
        let mut shape = None;
        for id in self.materials(scene, materials) {
            let index = self
                .handle(&scene.resources().materials()[id as usize])
                .unwrap()
                .raw() as usize;
            let texture = crate::texture::prepare_texture(&scene.resources().textures()[index])
                .map_err(|source| DrawError::Texture { index, source })?;
            if !self.supports(texture.format()) {
                return Err(crate::frame::FrameError::UnsupportedEffect(
                    "unsupported decal channel format",
                )
                .into());
            }
            let next = (
                texture.format(),
                texture.width(),
                texture.height(),
                texture.mips().len(),
            );
            if shape.is_some_and(|shape| shape != next) {
                return Err(crate::frame::FrameError::UnsupportedEffect(
                    "decal channel array requires matching format, dimensions and mips",
                )
                .into());
            }
            shape = Some(next);
        }
        Ok(())
    }
    fn supports(self, format: wgpu::TextureFormat) -> bool {
        use wgpu::TextureFormat as F;
        match self {
            Self::Albedo => matches!(
                format,
                F::Rgba8Unorm
                    | F::Rgba8UnormSrgb
                    | F::Bgra8Unorm
                    | F::Bgra8UnormSrgb
                    | F::Bc1RgbaUnorm
                    | F::Bc1RgbaUnormSrgb
                    | F::Bc3RgbaUnorm
                    | F::Bc3RgbaUnormSrgb
            ),
            Self::Normal => matches!(format, F::Bc5RgUnorm | F::Rg8Unorm),
            Self::Surface => matches!(format, F::Bc1RgbaUnorm | F::Bc3RgbaUnorm | F::Rgba8Unorm),
        }
    }
    pub fn texture(self, decals: &Decals, materials: &[u32]) -> wgpu::Texture {
        let materials = self.materials(&decals.scene, materials);
        if materials.is_empty() {
            return empty_texture(&decals.device);
        }
        let layers: Vec<_> = materials
            .iter()
            .map(|&id| {
                let material = &decals.scene.resources().materials()[id as usize];
                crate::texture::prepare_texture(
                    &decals.scene.resources().textures()
                        [self.handle(material).unwrap().raw() as usize],
                )
                .expect("validated decal channel")
            })
            .collect();
        crate::upload::upload_palette_array(
            &decals.device,
            &decals.queue,
            "visible decal channel",
            &layers,
        )
        .expect("validated bounded decal palette")
    }
    pub fn layer(self, scene: &RenderScene, materials: &[u32], id: u32) -> u32 {
        self.materials(scene, materials)
            .binary_search(&id)
            .map_or(0, |i| u32::try_from(i + 1).expect("bounded palette"))
    }
}
