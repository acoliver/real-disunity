use super::{LayeredError, LayeredReference};
use worldrunner_render_plan::{SceneTexture, TexelFormat, TextureColorSpace, TextureHandle};

impl LayeredReference<'_> {
    pub(super) fn material_sample(
        &self,
        appearance: &worldrunner_render_plan::SceneTerrainAppearance,
        uv: [f32; 2],
        derivatives: [[f32; 2]; 2],
        coverage: f32,
    ) -> Result<super::HeightLayerSample, LayeredError> {
        let layer = appearance.layered.ok_or(LayeredError::Source)?;
        let map = self
            .source
            .channels(layer, self.resources.textures())
            .map_err(|_| LayeredError::Texture)?;
        let values = [
            self.texture_filtered(layer.albedo_metallic, uv, derivatives)?,
            self.texture_filtered(layer.normal_roughness, uv, derivatives)?,
            self.texture_filtered(layer.height, uv, derivatives)?,
        ];
        let channel = |c: worldrunner_render_plan::TerrainLayerChannel| {
            values[c.texture.index()][usize::from(c.channel)]
        };
        let color = map.albedo.map(channel);
        let mut packed = [0.5; 4];
        let normal_count =
            if map.normal_encoding == worldrunner_render_plan::SceneNormalEncoding::TwoChannelXy {
                2
            } else {
                3
            };
        for (value, component) in packed.iter_mut().zip(map.normal).take(normal_count) {
            *value = channel(component);
        }
        let roughness = channel(map.roughness);
        let roughness = if map.invert_roughness {
            1.0 - roughness
        } else {
            roughness
        };
        let offsets =
            glam::IVec2::from_array(layer.roughness_metallic_offset_micro).as_vec2() * 0.000_001;
        Ok(super::HeightLayerSample {
            albedo: std::array::from_fn(|axis| {
                color[axis] * f32::from(appearance.tint_percent[axis]) / 100.0
            }),
            normal: super::decode_normal(packed, map.normal_encoding),
            roughness: roughness + offsets.x,
            metallic: channel(map.metallic) + offsets.y,
            height: channel(map.height),
            coverage,
        })
    }

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
        let SceneTexture::Texels {
            width,
            height,
            mips,
            ..
        } = texture
        else {
            return Err(LayeredError::Texture);
        };
        let expected = u32::BITS - width.max(height).leading_zeros();
        if *mips != 1 && *mips != expected {
            return Err(LayeredError::IncompleteMipChain {
                texture: handle.raw(),
                declared: *mips,
                expected,
            });
        }
        Ok(texture)
    }

    pub(super) fn texture(
        &self,
        handle: TextureHandle,
        uv: [f32; 2],
        repeat: bool,
    ) -> Result<[f32; 4], LayeredError> {
        self.texture_level(handle, uv, repeat, 0)
    }

    pub(super) fn texture_filtered(
        &self,
        handle: TextureHandle,
        uv: [f32; 2],
        derivatives: [[f32; 2]; 2],
    ) -> Result<[f32; 4], LayeredError> {
        let SceneTexture::Texels {
            width,
            height,
            mips,
            ..
        } = self.texels(handle)?
        else {
            unreachable!()
        };
        if *mips == 1 {
            return self.texture(handle, uv, true);
        }
        let dimensions = glam::UVec2::new(*width, *height).as_vec2();
        let footprint = derivatives.map(|d| (glam::Vec2::from_array(d) * dimensions).length());
        let maximum = f32::from(u16::try_from(*mips - 1).expect("validated mip count"));
        let lod = footprint[0].max(footprint[1]).max(1.0).log2().min(maximum);
        let lower = (0..*mips)
            .rfind(|level| f64::from(*level) <= f64::from(lod))
            .unwrap();
        let a = self.texture_level(handle, uv, true, lower)?;
        let b = self.texture_level(handle, uv, true, (lower + 1).min(*mips - 1))?;
        Ok(std::array::from_fn(|axis| {
            a[axis] + (b[axis] - a[axis]) * lod.fract()
        }))
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        reason = "reference uses GPU f32 coordinate precision and floored integer texel positions"
    )]
    fn texture_level(
        &self,
        handle: TextureHandle,
        uv: [f32; 2],
        repeat: bool,
        level: u32,
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
        let channels = usize::try_from(format.bytes_per_texel()).expect("texture channels");
        let offset: usize = (0..level)
            .map(|mip| {
                ((*width >> mip).max(1) as usize) * ((*height >> mip).max(1) as usize) * channels
            })
            .sum();
        let image = Image {
            format: *format,
            color_space: *color_space,
            dimensions: [(*width >> level).max(1), (*height >> level).max(1)],
            bytes: &bytes[offset..],
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
