//! Counts of the render textures a dispatch reads, taken from the GPU as the dispatch is recorded:
//! once after the operations that precede it, which end with the camera passes that drew them, and
//! once in the dispatch's own recording, right before the dispatch. Counts and ranges only.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::runtime::{PendingTargetRead, ProgramRuntime, TextureHandle};
use worldrunner_render_program::texels::texel_stats;
use worldrunner_render_program::unity::offscreen::{
    OffscreenOperations, OffscreenState, PendingImage,
};

use super::compute::DispatchTrace;

/// A render texture a dispatch binds, by its place in the dispatch trace's textures.
struct Probed {
    texture: usize,
    id: TextureId,
    handle: TextureHandle,
}

/// What to read back of the render textures one dispatch binds.
pub(super) struct Probe {
    textures: Vec<Probed>,
    after_operations: Vec<(usize, PendingImage)>,
}

/// The reads recorded into the dispatch's frame, to resolve once it is submitted.
pub(super) struct BeforeDispatch(Vec<(usize, PendingTargetRead)>);

impl Probe {
    /// Plans the reads of the render textures `trace` says the dispatch binds, and appends the
    /// first of them to `operations`.
    pub(super) fn plan(
        trace: &DispatchTrace,
        state: &OffscreenState,
        mut operations: Option<&mut OffscreenOperations>,
    ) -> Self {
        let mut textures = Vec::new();
        let mut after_operations = Vec::new();
        for (index, texture) in trace.textures.iter().enumerate() {
            let (Some(id), "render") = (texture.texture, texture.source.as_str()) else {
                continue;
            };
            let Some(handle) = state.texture_handle(id) else {
                continue;
            };
            textures.push(Probed {
                texture: index,
                id,
                handle,
            });
            if let Some(operations) = operations.as_deref_mut()
                && let Ok(image) = operations.read_back(id, 0, 0)
            {
                after_operations.push((index, image));
            }
        }
        Self {
            textures,
            after_operations,
        }
    }

    /// Records the second read of each texture into the dispatch's frame.
    pub(super) fn record(
        &self,
        runtime: &ProgramRuntime,
        frame: &mut wgpu::CommandEncoder,
    ) -> BeforeDispatch {
        BeforeDispatch(
            self.textures
                .iter()
                .filter_map(|probed| {
                    let read = runtime.read_texture(frame, probed.handle).ok()?;
                    Some((probed.texture, read))
                })
                .collect(),
        )
    }

    /// Resolves the reads once the frame is submitted and writes what they hold into `trace`.
    pub(super) fn resolve(
        self,
        before: BeforeDispatch,
        submission: &wgpu::SubmissionIndex,
        device: &wgpu::Device,
        trace: &mut DispatchTrace,
    ) {
        for (index, image) in self.after_operations {
            let format = image.format();
            let bytes = image.bytes(device);
            if let Some(entry) = trace.textures.get_mut(index) {
                entry.after_operations = Some(texel_stats(&bytes, format).describe());
            }
        }
        for (index, read) in before.0 {
            let Ok(image) = read.resolve(submission.clone()) else {
                continue;
            };
            if let Some(entry) = trace.textures.get_mut(index) {
                entry.before_dispatch = Some(texel_stats(&image.bytes, image.format).describe());
            }
        }
        for probed in &self.textures {
            if let Some(entry) = trace.textures.get_mut(probed.texture) {
                entry
                    .after_operations
                    .get_or_insert_with(|| "not read".to_owned());
                entry
                    .before_dispatch
                    .get_or_insert_with(|| "not read".to_owned());
                debug_assert_eq!(entry.texture, Some(probed.id));
            }
        }
    }
}
