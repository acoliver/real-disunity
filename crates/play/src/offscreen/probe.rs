//! Counts of the render textures a dispatch reads, taken from the GPU as the dispatch is recorded:
//! once after the operations that precede it, which end with the camera passes that drew them, and
//! once in the dispatch's own recording, right before the dispatch. Counts and ranges only.

use worldrunner_render_program::ids::TextureId;
use worldrunner_render_program::runtime::{PendingTargetRead, ProgramRuntime, TextureHandle};
use worldrunner_render_program::texels::{texel_stats, texels_differing};
use worldrunner_render_program::unity::offscreen::{
    OffscreenOperations, OffscreenState, PendingImage,
};

use super::compute::DispatchTrace;
use super::frame::{TEST_CLEAR, TEST_VARIANTS};
use super::ids::{probe_scratch, probe_variant};

/// A render texture a dispatch binds, by its place in the dispatch trace's textures.
struct Probed {
    texture: usize,
    id: TextureId,
    handle: TextureHandle,
}

/// An uploaded texture a camera render sampled, by its place in the dispatch trace: the texture's
/// index and the index of the source in the texture's sampled list.
struct ProbedSource {
    texture: usize,
    source: usize,
    handle: TextureHandle,
}

/// What to read back of the render textures one dispatch binds.
pub(super) struct Probe {
    textures: Vec<Probed>,
    after_operations: Vec<(usize, PendingImage)>,
    /// The test renders' scratch copies, read after the operations, by the place of the texture
    /// in the trace and the render: 0 as declared, else the place in [`TEST_VARIANTS`] plus one.
    test_renders: Vec<(usize, usize, PendingImage)>,
    sources: Vec<ProbedSource>,
}

/// The reads recorded into the dispatch's frame, to resolve once it is submitted.
pub(super) struct BeforeDispatch {
    textures: Vec<(usize, PendingTargetRead)>,
    sources: Vec<(usize, usize, PendingTargetRead)>,
}

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
        let mut test_renders = Vec::new();
        let mut sources = Vec::new();
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
            let scratches = std::iter::once(probe_scratch(id)).chain(
                (0..)
                    .take(TEST_VARIANTS.len())
                    .map(|number| probe_variant(id, number)),
            );
            for (place, scratch) in scratches.enumerate() {
                if let Some(operations) = operations.as_deref_mut()
                    && state.texture_handle(scratch).is_some()
                    && let Ok(image) = operations.read_back(scratch, 0, 0)
                {
                    test_renders.push((index, place, image));
                }
            }
            for (at, source) in texture.sampled.iter().enumerate() {
                if let Some(handle) = state.texture_handle(source.texture) {
                    sources.push(ProbedSource {
                        texture: index,
                        source: at,
                        handle,
                    });
                }
            }
        }
        Self {
            textures,
            after_operations,
            test_renders,
            sources,
        }
    }

    /// Records the second read of each texture into the dispatch's frame.
    pub(super) fn record(
        &self,
        runtime: &ProgramRuntime,
        frame: &mut wgpu::CommandEncoder,
    ) -> BeforeDispatch {
        BeforeDispatch {
            textures: self
                .textures
                .iter()
                .filter_map(|probed| {
                    let read = runtime.read_texture(frame, probed.handle).ok()?;
                    Some((probed.texture, read))
                })
                .collect(),
            sources: self
                .sources
                .iter()
                .filter_map(|probed| {
                    let read = runtime.read_texture(frame, probed.handle).ok()?;
                    Some((probed.texture, probed.source, read))
                })
                .collect(),
        }
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
        for (index, place, image) in self.test_renders {
            let (format, bytes) = (image.format(), image.bytes(device));
            if let (Some(entry), Some(differing)) = (
                trace.textures.get_mut(index),
                texels_differing(&bytes, format, TEST_CLEAR),
            ) {
                let texels = texel_stats(&bytes, format).texels;
                let words = format!(
                    "{differing} of {texels} texels differ from the clear ({TEST_CLEAR:?})"
                );
                match place.checked_sub(1) {
                    None => entry.test_render = Some(words),
                    Some(variant) => entry
                        .variant_renders
                        .push(format!("{}: {words}", TEST_VARIANTS[variant].0)),
                }
            }
        }
        for (index, source, read) in before.sources {
            let Ok(image) = read.resolve(submission.clone()) else {
                continue;
            };
            let stats = texel_stats(&image.bytes, image.format);
            if let Some(entry) = trace
                .textures
                .get_mut(index)
                .and_then(|texture| texture.sampled.get_mut(source))
            {
                entry.on_the_gpu = Some(format!("{} non-zero", stats.nonzero));
            }
        }
        for (index, read) in before.textures {
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
