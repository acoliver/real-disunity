use super::{LayeredError, LayeredReference};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace, TextureHandle};

impl LayeredReference<'_> {
    pub(super) fn validate_textures(&self) -> Result<(), LayeredError> {
        for surface in self.terrain.surfaces() {
            let layer = surface.appearance().layered.ok_or(LayeredError::Source)?;
            for handle in [layer.albedo_metallic, layer.normal_roughness, layer.height] {
                self.texels(handle)?;
            }
        }
        if let Some(map) = self.source.colorization {
            self.texels(map.texture)?;
        }
        Ok(())
    }

    fn texels(&self, handle: TextureHandle) -> Result<&SceneTexture, LayeredError> {
        let mut texture = self
            .resources
            .textures()
            .get(handle.raw() as usize)
            .ok_or(LayeredError::Texture)?;
        while let SceneTexture::Sampled { texture: inner, .. } = texture {
            texture = inner;
        }
        if !matches!(texture, SceneTexture::Texels { .. }) {
            return Err(LayeredError::Texture);
        }
        Ok(texture)
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "reference uses GPU f32 coordinate precision and floored integer texel positions"
    )]
    pub(super) fn texture(
        &self,
        handle: TextureHandle,
        uv: [f32; 2],
        repeat: bool,
    ) -> Result<[f32; 4], LayeredError> {
        let SceneTexture::Texels {
            format,
            color_space,
            width,
            height,
            bytes,
            ..
        } = self.texels(handle)?
        else {
            unreachable!()
        };
        let image = Image {
            format: *format,
            color_space: *color_space,
            dimensions: [*width, *height],
            bytes,
        };
        let position: [f32; 2] = std::array::from_fn(|axis| {
            let coordinate = if repeat {
                uv[axis].rem_euclid(1.0)
            } else {
                uv[axis].clamp(0.0, 1.0)
            };
            coordinate * image.dimensions[axis] as f32 - 0.5
        });
        let base = position.map(|x| x.floor() as i64);
        let fraction = position.map(|x| x - x.floor());
        let a = image.read(base, repeat);
        let b = image.read([base[0] + 1, base[1]], repeat);
        let c = image.read([base[0], base[1] + 1], repeat);
        let d = image.read([base[0] + 1, base[1] + 1], repeat);
        Ok(std::array::from_fn(|axis| {
            let lower = a[axis] + (b[axis] - a[axis]) * fraction[0];
            let upper = c[axis] + (d[axis] - c[axis]) * fraction[0];
            lower + (upper - lower) * fraction[1]
        }))
    }
}

struct Image<'a> {
    format: TexelFormat,
    color_space: TextureColorSpace,
    dimensions: [u32; 2],
    bytes: &'a [u8],
}

impl Image<'_> {
    fn read(&self, at: [i64; 2], repeat: bool) -> [f32; 4] {
        let at: [usize; 2] = std::array::from_fn(|axis| {
            let size = i64::from(self.dimensions[axis]);
            let index = if repeat {
                at[axis].rem_euclid(size)
            } else {
                at[axis].clamp(0, size - 1)
            };
            usize::try_from(index).expect("validated image extent")
        });
        let channels =
            usize::try_from(self.format.bytes_per_texel()).expect("at most four channels");
        let index = (at[1] * self.dimensions[0] as usize + at[0]) * channels;
        let mut result = [0.0, 0.0, 0.0, 1.0];
        for (value, byte) in result.iter_mut().zip(&self.bytes[index..index + channels]) {
            *value = f32::from(*byte) / 255.0;
        }
        if self.format == TexelFormat::Bgra8Unorm {
            result.swap(0, 2);
        }
        if self.color_space == TextureColorSpace::Srgb {
            for value in &mut result[..3] {
                *value = if *value <= 0.04045 {
                    *value / 12.92
                } else {
                    ((*value + 0.055) / 1.055).powf(2.4)
                };
            }
        }
        result
    }
}
