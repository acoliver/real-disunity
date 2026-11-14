//! The scene palette: every pose's current and previous palettes in one GPU buffer.

use worldrunner_render_plan::{SceneSkinPose, SkinPoseHandle};

use super::{PALETTE_ENTRY_BYTES, extend_palette};

/// Where one pose's palettes sit in the scene palette, counted in entries (joints).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PoseRange {
    pub current: u32,
    pub previous: u32,
    pub joints: u32,
}

/// The palette buffer's contents and the range of each pose, indexed by [`SkinPoseHandle`].
#[derive(Debug, Default)]
pub(crate) struct ScenePalette {
    bytes: Vec<u8>,
    ranges: Vec<PoseRange>,
}

/// The scene's palettes do not fit the 32-bit entry offsets the shader indexes with.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("skin palettes need {entries} entries, beyond the 32-bit palette index")]
pub(crate) struct PaletteOverflow {
    pub entries: u64,
}

impl ScenePalette {
    /// Lays out `poses` in handle order, each as its current palette then its previous one.
    pub(crate) fn new(poses: &[SceneSkinPose]) -> Result<Self, PaletteOverflow> {
        let entries: u64 = poses.iter().map(|pose| 2 * pose.joint_count() as u64).sum();
        let overflow = PaletteOverflow { entries };
        u32::try_from(entries).map_err(|_| overflow)?;
        let mut palette = Self {
            bytes: Vec::with_capacity(usize::try_from(entries * PALETTE_ENTRY_BYTES).unwrap_or(0)),
            ranges: Vec::with_capacity(poses.len()),
        };
        let mut next = 0_u32;
        for pose in poses {
            // Both conversions are bounded by the checked total above.
            let joints = pose.joint_count() as u32;
            palette.ranges.push(PoseRange {
                current: next,
                previous: next + joints,
                joints,
            });
            extend_palette(&mut palette.bytes, pose.current());
            extend_palette(&mut palette.bytes, pose.previous());
            next += 2 * joints;
        }
        Ok(palette)
    }

    /// The buffer bytes, three `vec4<f32>` rows per entry.
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The range of `pose`, or `None` for a handle outside the scene's pose table.
    pub(crate) fn range(&self, pose: SkinPoseHandle) -> Option<PoseRange> {
        self.ranges.get(pose.raw() as usize).copied()
    }

    /// The per-instance descriptor `(current, previous, joints, flags)` of `pose`.
    ///
    /// An instance without motion history (first drawn frame, teleport or animation reset)
    /// reads its current palette as the previous one, so skinning adds no motion to it.
    pub(crate) fn descriptor(&self, pose: SkinPoseHandle, history: bool) -> Option<[u32; 4]> {
        let range = self.range(pose)?;
        let previous = if history {
            range.previous
        } else {
            range.current
        };
        Some([range.current, previous, range.joints, 0])
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
        let start = entry as usize * PALETTE_ENTRY_BYTES as usize + 12;
        f32::from_le_bytes(palette.bytes()[start..start + 4].try_into().unwrap())
    }

    #[test]
    fn poses_lay_out_current_then_previous_in_handle_order() {
        let palette = ScenePalette::new(&[
            pose(&[1_000, 2_000], &[3_000, 4_000]),
            pose(&[5_000], &[6_000]),
        ])
        .unwrap();
        let first = palette.range(SkinPoseHandle::from_raw(0)).unwrap();
        let second = palette.range(SkinPoseHandle::from_raw(1)).unwrap();
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
        let palette = ScenePalette::new(&[pose(&[0; 3], &[0; 3])]).unwrap();
        let handle = SkinPoseHandle::from_raw(0);
        assert_eq!(palette.descriptor(handle, true), Some([0, 3, 3, 0]));
        assert_eq!(palette.descriptor(handle, false), Some([0, 0, 3, 0]));
    }

    #[test]
    fn a_handle_outside_the_pose_table_has_no_range() {
        let palette = ScenePalette::new(&[pose(&[0], &[0])]).unwrap();
        assert_eq!(palette.descriptor(SkinPoseHandle::from_raw(1), true), None);
    }
}
