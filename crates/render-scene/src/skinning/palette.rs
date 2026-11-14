//! The scene palette: every pose's current and previous palettes in one GPU buffer.

use worldrunner_render_plan::SceneSkinPose;

use super::{PALETTE_ENTRY_BYTES, extend_palette};

/// Where one pose's palettes sit in the scene palette, counted in entries (joints).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PoseRange {
    pub current: u32,
    pub previous: u32,
    pub joints: u32,
}

impl PoseRange {
    /// The per-instance descriptor `(current, previous, joints, flags)` of the pose.
    ///
    /// An instance without motion history (first drawn frame, teleport or animation reset)
    /// reads its current palette as the previous one, so skinning adds no motion to it.
    pub(crate) fn descriptor(self, history: bool) -> [u32; 4] {
        let previous = if history { self.previous } else { self.current };
        [self.current, previous, self.joints, 0]
    }
}

/// The palette buffer's contents. The range of each pose, indexed by the pose handle, comes
/// from [`ScenePalette::ranges_of`] for the same poses.
#[derive(Debug, Default)]
pub(crate) struct ScenePalette {
    bytes: Vec<u8>,
}

/// The scene's palettes do not fit the 32-bit entry offsets the shader indexes with.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("skin palettes need {entries} entries, beyond the 32-bit palette index")]
pub(crate) struct PaletteOverflow {
    pub entries: u64,
}

impl ScenePalette {
    /// The entries `poses` occupy, or the overflow of the 32-bit entry index.
    pub(crate) fn entries(poses: &[SceneSkinPose]) -> Result<u32, PaletteOverflow> {
        let entries: u64 = poses.iter().map(|pose| 2 * pose.joint_count() as u64).sum();
        u32::try_from(entries).map_err(|_| PaletteOverflow { entries })
    }

    /// The range of each pose when `poses` are laid out in handle order.
    pub(crate) fn ranges_of(poses: &[SceneSkinPose]) -> Result<Vec<PoseRange>, PaletteOverflow> {
        Self::entries(poses)?;
        let mut next = 0_u32;
        let mut ranges = Vec::with_capacity(poses.len());
        for pose in poses {
            let joints = u32::try_from(pose.joint_count()).map_err(|_| PaletteOverflow {
                entries: pose.joint_count() as u64,
            })?;
            ranges.push(PoseRange {
                current: next,
                previous: next + joints,
                joints,
            });
            // The total was checked above, so the running offset cannot pass it.
            next += 2 * joints;
        }
        Ok(ranges)
    }

    /// Lays out `poses` in handle order, each as its current palette then its previous one.
    pub(crate) fn new(poses: &[SceneSkinPose]) -> Result<Self, PaletteOverflow> {
        let entries = u64::from(Self::entries(poses)?);
        let mut bytes =
            Vec::with_capacity(usize::try_from(entries * PALETTE_ENTRY_BYTES).unwrap_or(0));
        for pose in poses {
            extend_palette(&mut bytes, pose.current());
            extend_palette(&mut bytes, pose.previous());
        }
        Ok(Self { bytes })
    }

    /// Whether there are no entries, so there is nothing to bind.
    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// The buffer bytes, three `vec4<f32>` rows per entry.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use worldrunner_render_plan::SceneAffineMatrix;

    use super::*;

    fn shifted(millimeters: i64) -> SceneAffineMatrix {
        SceneAffineMatrix {
            translation_millimeters: [millimeters, 0, 0],
            ..SceneAffineMatrix::IDENTITY
        }
    }

    fn pose(current: &[i64], previous: &[i64]) -> SceneSkinPose {
        SceneSkinPose::new(
            current.iter().copied().map(shifted).collect(),
            previous.iter().copied().map(shifted).collect(),
        )
        .unwrap()
    }

    /// The x translation, in metres, of palette entry `entry`.
    fn entry_x(palette: &ScenePalette, entry: u32) -> f32 {
        let entry_bytes = usize::try_from(PALETTE_ENTRY_BYTES).unwrap();
        let start = entry as usize * entry_bytes + 12;
        f32::from_le_bytes(palette.bytes()[start..start + 4].try_into().unwrap())
    }

    #[test]
    fn poses_lay_out_current_then_previous_in_handle_order() {
        let poses = [
            pose(&[1_000, 2_000], &[3_000, 4_000]),
            pose(&[5_000], &[6_000]),
        ];
        let palette = ScenePalette::new(&poses).unwrap();
        let ranges = ScenePalette::ranges_of(&poses).unwrap();
        let (first, second) = (ranges[0], ranges[1]);
        assert_eq!(
            first,
            PoseRange {
                current: 0,
                previous: 2,
                joints: 2
            }
        );
        assert_eq!(
            second,
            PoseRange {
                current: 4,
                previous: 5,
                joints: 1
            }
        );
        let metres: Vec<f32> = (0..6).map(|entry| entry_x(&palette, entry)).collect();
        assert_eq!(metres, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        assert_eq!(palette.bytes().len() as u64, 6 * PALETTE_ENTRY_BYTES);
    }

    #[test]
    fn an_instance_without_history_reads_its_current_palette_as_previous() {
        let ranges = ScenePalette::ranges_of(&[pose(&[0; 3], &[0; 3])]).unwrap();
        assert_eq!(ranges[0].descriptor(true), [0, 3, 3, 0]);
        assert_eq!(ranges[0].descriptor(false), [0, 0, 3, 0]);
    }
}
