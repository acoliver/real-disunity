//! Authored linear BC6H or half-float reflection radiance, optionally filtered on installation.
use super::SceneValidationError;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SceneSpecularCubeFormat {
    Bc6hRgbUfloat,
    Rgba16Float,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneSpecularCube {
    preparation: Option<super::SceneSpecularPreparation>,
    size: u32,
    mips: u32,
    format: SceneSpecularCubeFormat,
    bytes: Box<[u8]>,
}
impl SceneSpecularCube {
    /// Six faces in +X, -X, +Y, -Y, +Z, -Z order. Each face contains its full
    /// mip chain from largest to smallest; each 4x4 block occupies sixteen bytes.
    pub fn new(size: u32, bytes: Box<[u8]>) -> Result<Self, SceneValidationError> {
        if !(4..=1024).contains(&size) || !size.is_power_of_two() {
            return Err(SceneValidationError::UnsupportedInput {
                part: "specular cube size",
            });
        }
        let expected: u64 = (0..=size.ilog2())
            .map(|mip| level_bytes(size >> mip))
            .sum::<u64>()
            * 6;
        if u64::try_from(bytes.len()).expect("addressable cube bytes") != expected {
            return Err(SceneValidationError::UnsupportedInput {
                part: "specular cube mip payload",
            });
        }
        Ok(Self {
            preparation: None,
            size,
            mips: size.ilog2() + 1,
            format: SceneSpecularCubeFormat::Bc6hRgbUfloat,
            bytes,
        })
    }
    /// Linear half-float RGBA, face-major with finest-to-coarsest mips per face.
    pub fn rgba16_float(
        size: u32,
        mips: u32,
        bytes: Box<[u8]>,
    ) -> Result<Self, SceneValidationError> {
        if size == 0
            || size > 1024
            || !size.is_power_of_two()
            || mips == 0
            || mips > size.ilog2() + 1
        {
            return Err(SceneValidationError::UnsupportedInput {
                part: "specular cube extent or mips",
            });
        }
        let expected: u64 = (0..mips)
            .map(|m| u64::from(size >> m).pow(2) * 8)
            .sum::<u64>()
            * 6;
        if bytes.len() as u64 != expected {
            return Err(SceneValidationError::UnsupportedInput {
                part: "specular cube mip payload",
            });
        }
        Ok(Self {
            preparation: None,
            size,
            mips,
            format: SceneSpecularCubeFormat::Rgba16Float,
            bytes,
        })
    }
    /// Request host-side filtering of these source bytes instead of uploading
    /// them as an already prefiltered result. Other cube constructors stay unchanged.
    pub fn with_preparation(
        mut self,
        settings: super::SceneSpecularPreparation,
    ) -> Result<Self, SceneValidationError> {
        settings.validate()?;
        self.preparation = Some(settings);
        Ok(self)
    }
    pub const fn preparation(&self) -> Option<super::SceneSpecularPreparation> {
        self.preparation
    }
    pub const fn format(&self) -> SceneSpecularCubeFormat {
        self.format
    }
    fn level_bytes(&self, size: u32) -> u64 {
        match self.format {
            SceneSpecularCubeFormat::Bc6hRgbUfloat => level_bytes(size),
            SceneSpecularCubeFormat::Rgba16Float => u64::from(size).pow(2) * 8,
        }
    }
    pub const fn size(&self) -> u32 {
        self.size
    }
    pub const fn mip_count(&self) -> u32 {
        self.mips
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn subresource(&self, face: u32, mip: u32) -> Option<&[u8]> {
        if face >= 6 || mip >= self.mip_count() {
            return None;
        }
        let face_bytes: u64 = (0..self.mip_count())
            .map(|m| self.level_bytes(self.size >> m))
            .sum();
        let preceding: u64 = (0..mip).map(|m| self.level_bytes(self.size >> m)).sum();
        let start =
            usize::try_from(u64::from(face) * face_bytes + preceding).expect("bounded cube");
        let length =
            usize::try_from(self.level_bytes(self.size >> mip)).expect("bounded cube level");
        Some(&self.bytes[start..start + length])
    }
}
fn level_bytes(size: u32) -> u64 {
    u64::from(size.div_ceil(4)).pow(2) * 16
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn face_major_full_mip_order_matches_independent_block_counts() {
        let mut bytes = vec![];
        for face in 0_u8..6 {
            for (mip, count) in [(0_u8, 64), (1, 16), (2, 16), (3, 16)] {
                bytes.extend(vec![face * 16 + mip; count]);
            }
        }
        let cube = SceneSpecularCube::new(8, bytes.clone().into_boxed_slice()).unwrap();
        assert_eq!(cube.bytes(), bytes);
        assert_eq!(cube.mip_count(), 4);
        for face in 0_u8..6 {
            for (mip, count) in [(0_u8, 64), (1, 16), (2, 16), (3, 16)] {
                assert_eq!(
                    cube.subresource(u32::from(face), u32::from(mip)).unwrap(),
                    vec![face * 16 + mip; count]
                );
            }
        }
        assert!(cube.subresource(6, 0).is_none());
        assert!(cube.subresource(0, 4).is_none());
    }
    #[test]
    fn malformed_sizes_and_incomplete_or_surplus_mips_fail() {
        for size in [0, 1, 2, 6, 2048] {
            assert!(SceneSpecularCube::new(size, vec![0; 288].into_boxed_slice()).is_err());
        }
        for length in [0, 96, 287, 289, 384] {
            assert!(SceneSpecularCube::new(4, vec![0; length].into_boxed_slice()).is_err());
        }
        assert!(SceneSpecularCube::new(4, vec![0; 288].into_boxed_slice()).is_ok());
    }
}
