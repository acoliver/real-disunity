//! Retained contour membership and the original-range object-program draw.
use super::{SceneGpu, visibility::SelectedRange};
use crate::object::program::Context;
use std::collections::{BTreeMap, BTreeSet};
use worldrunner_render_plan::{SceneContours, SceneValidationError};
use worldrunner_render_post::FrameError;
use worldrunner_render_program::runtime::PassCommands;

pub(crate) struct ContourDraw {
    context: Context,
    groups: BTreeMap<u8, wgpu::BindGroup>,
}

impl SceneGpu {
    /// Replaces styles atomically, rejecting removal of IDs used by any live placement.
    pub fn set_contours(&mut self, contours: SceneContours) -> Result<(), SceneValidationError> {
        self.instances.set_contours(contours)
    }

    pub fn contours(&self) -> &SceneContours {
        &self.instances.contours
    }

    pub fn contour_member_count(&self) -> usize {
        self.instances.contour_members.len()
    }

    pub(crate) fn prepare_contours(
        &self,
        pipelines: &crate::renderer::ScenePipelines,
        frame: &crate::frame::FrameUniform,
    ) -> Result<ContourDraw, FrameError> {
        let context = self
            .selected_pipelines(pipelines, frame)
            .object_context()
            .contour();
        let keys: BTreeSet<_> = self
            .instances
            .contour_members
            .keys()
            .map(|(key, _)| *key)
            .collect();
        for (pipeline, mesh, material) in keys {
            for (level, slots) in self.mesh_selection[mesh].material_slots.iter().enumerate() {
                let geometry = self.meshes[mesh].lods[level].layout;
                let pairs: Vec<_> = slots.as_ref().map_or_else(
                    || vec![(pipeline, material)],
                    |slots| {
                        slots
                            .iter()
                            .map(|slot| {
                                let material = slot.material().raw() as usize;
                                (self.instances.pipelines[material], material)
                            })
                            .collect()
                    },
                );
                for pair in pairs {
                    self.objects
                        .pipeline(&context, pair, geometry)
                        .map_err(|error| FrameError::FeaturePreparation {
                            feature: "contour IDs",
                            reason: error.to_string(),
                        })?;
                }
            }
        }
        let groups = self
            .instances
            .contour_members
            .values()
            .map(|&id| (id, context.contour_group(id)))
            .collect();
        Ok(ContourDraw { context, groups })
    }

    pub(crate) fn draw_contours<P: PassCommands + ?Sized>(&self, pass: &mut P, draw: &ContourDraw) {
        let frame = self.drawing_uniform();
        let selection = self.select_objects(None, &frame);
        let mut drawn = super::ObjectDraws::new(super::ObjectRecording::Ranged);
        pass.set_bind_group(0, &self.drawing_frame_group());
        for range in &selection.ranges {
            for (id, selected) in intersections(range, &self.instances.contour_members) {
                pass.set_bind_group(2, &draw.groups[&id]);
                let mut scratch = self.sink_scratch.borrow_mut();
                let mut sink = super::object_runtime::PreparedSink::new(
                    pass,
                    &self.objects,
                    &draw.context,
                    &mut scratch,
                );
                // The camera's compacted instance indices do not address original skin streams.
                self.record_lod_range(&mut sink, &selected, &mut drawn);
                sink.finish();
            }
        }
    }
}

fn intersections(
    selected: &SelectedRange,
    members: &BTreeMap<(crate::instances::BatchKey, usize), u8>,
) -> Vec<(u8, SelectedRange)> {
    let mut runs: Vec<(u8, SelectedRange)> = Vec::new();
    for (&(_, slot), &id) in members.range(
        (selected.key, selected.slots.start as usize)..(selected.key, selected.slots.end as usize),
    ) {
        let slot = u32::try_from(slot).expect("validated instance slot");
        if let Some((_, last)) = runs
            .last_mut()
            .filter(|(last_id, last)| *last_id == id && last.slots.end == slot)
        {
            last.slots.end += 1;
        } else {
            runs.push((
                id,
                SelectedRange {
                    slots: slot..slot + 1,
                    ..selected.clone()
                },
            ));
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contour_members_intersect_original_camera_ranges_and_split_by_id() {
        let range = SelectedRange {
            key: (0, 2, 1),
            lod: 3,
            slots: 2..8,
        };
        let mut members = BTreeMap::new();
        for (slot, id) in [(0, 1), (2, 255), (3, 255), (4, 1), (6, 1), (8, 1)] {
            members.insert((range.key, slot), id);
        }
        members.insert(((1, 2, 1), 3), 1);
        let runs = intersections(&range, &members);
        assert_eq!(
            runs.iter()
                .map(|(id, range)| (*id, range.slots.clone()))
                .collect::<Vec<_>>(),
            vec![(255, 2..4), (1, 4..5), (1, 6..7)]
        );
        assert!(
            runs.iter()
                .all(|(_, selected)| selected.key == range.key && selected.lod == range.lod)
        );
    }
}
