//! Authored linear BC6H unsigned-float reflection radiance.
use super::SceneValidationError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SceneSpecularCube {
    size: u32,
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
        Ok(Self { size, bytes })
    }
    pub const fn size(&self) -> u32 {
        self.size
    }
    pub const fn mip_count(&self) -> u32 {
        self.size.ilog2() + 1
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn subresource(&self, face: u32, mip: u32) -> Option<&[u8]> {
        if face >= 6 || mip >= self.mip_count() {
            return None;
        }
        let face_bytes: u64 = (0..self.mip_count())
            .map(|m| level_bytes(self.size >> m))
            .sum();
        let preceding: u64 = (0..mip).map(|m| level_bytes(self.size >> m)).sum();
        let start =
            usize::try_from(u64::from(face) * face_bytes + preceding).expect("bounded cube");
        let length = usize::try_from(level_bytes(self.size >> mip)).expect("bounded cube level");
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
