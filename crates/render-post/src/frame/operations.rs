//! Operations groups: work a host records into the frame's encoder between the authored passes.

use super::{AttachmentLoad, FrameError, PassDeclaration, PassEntry, Product, ResourceRef};

/// Work that records commands of its own into the frame's encoder, outside the raster passes
/// of the plan: a checked operations list the host admitted, for one.
pub trait FrameOperations {
    /// What the work leaves written for the passes that follow it.
    fn produces(&self) -> &[OperationsOutput] {
        &[]
    }
    /// Records the work into `encoder`. `timing` is the frame's timer; work that lists itself
    /// in the frame's timing (as `PassId::Operation`) does so through it.
    ///
    /// # Errors
    /// Whatever refuses the work, for example a resource replaced since admission. The frame
    /// is abandoned.
    fn encode_operations(
        &self,
        timing: Option<&crate::TimingState>,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError>;
}

/// A texture subresource the operations leave written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OperationsOutput {
    pub resource: super::ResourceRef,
    pub format: wgpu::TextureFormat,
    pub size: [u32; 2],
}
impl OperationsOutput {
    pub const fn new(
        resource: super::ResourceRef,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) -> Self {
        Self {
            resource,
            format,
            size,
        }
    }
}

/// An operations group and where it runs in the authored pass list.
#[derive(Clone, Copy)]
pub struct OperationsGroup<'a> {
    /// The entry the group runs immediately before. `None` runs it after every pass.
    pub before: Option<PassEntry>,
    pub work: &'a dyn FrameOperations,
}
impl std::fmt::Debug for OperationsGroup<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("OperationsGroup")
            .field("before", &self.before)
            .finish_non_exhaustive()
    }
}

/// The index of the pass a group runs before, `passes.len()` for a group that runs after every
/// pass (or before an entry the plan does not author, which `validate_operations` refuses).
fn position(passes: &[PassDeclaration], group: &OperationsGroup<'_>) -> usize {
    group.before.map_or(passes.len(), |entry| {
        passes
            .iter()
            .position(|pass| pass.entry() == entry)
            .unwrap_or(passes.len())
    })
}

/// Refuses a group positioned before an entry the plan does not author, and an output that the
/// plan's views do not hand to the executor as a single-sample view of the same format and size
/// with retained contents.
pub(crate) fn validate_operations(
    passes: &[PassDeclaration],
    views: &[super::AttachmentView],
    operations: &[OperationsGroup<'_>],
) -> Result<(), FrameError> {
    for entry in operations.iter().filter_map(|group| group.before) {
        if !passes.iter().any(|pass| pass.entry() == entry) {
            return Err(FrameError::MissingPass(entry));
        }
    }
    for output in operations.iter().flat_map(|group| group.work.produces()) {
        let hosted = views
            .iter()
            .find(|view| view.resource() == output.resource)
            .is_some_and(|view| {
                view.retained()
                    && view.format() == output.format
                    && view.size() == output.size
                    && view.texture().sample_count() == 1
            });
        if !hosted {
            return Err(FrameError::AttachmentView(output.resource));
        }
    }
    Ok(())
}

/// What the groups of a plan leave written, with the index of the pass each group runs before.
pub(crate) struct GroupOutputs(Vec<(usize, OperationsOutput)>);
impl GroupOutputs {
    pub(crate) fn new(passes: &[PassDeclaration], operations: &[OperationsGroup<'_>]) -> Self {
        Self(
            operations
                .iter()
                .flat_map(|group| {
                    let at = position(passes, group);
                    group
                        .work
                        .produces()
                        .iter()
                        .map(move |output| (at, *output))
                })
                .collect(),
        )
    }

    /// Whether some group writes `resource`. A retained view of it has contents only from that
    /// group's position on.
    pub(crate) fn writes(&self, resource: ResourceRef) -> bool {
        self.0.iter().any(|(_, output)| output.resource == resource)
    }

    /// The outputs of the groups that run immediately before pass `index`, as the producers
    /// later passes read.
    pub(super) fn left_before(
        &self,
        index: usize,
    ) -> impl Iterator<Item = (ResourceRef, Product)> + '_ {
        self.0
            .iter()
            .filter(move |(at, _)| *at == index)
            .map(|(_, output)| {
                (
                    output.resource,
                    Product {
                        format: output.format,
                        size: output.size,
                        samples: 1,
                        store: wgpu::StoreOp::Store,
                    },
                )
            })
    }

    /// Refuses an attachment of `pass` that is an output, unless it is a depth attachment that
    /// loads without writing: the one way a pass attaches a texture it only reads. A compute
    /// pass may not write an output either.
    pub(crate) fn refuse_writes(&self, pass: &PassDeclaration) -> Result<(), FrameError> {
        let attachments = pass
            .color
            .iter()
            .chain(&pass.additional_colors)
            .chain(&pass.depth);
        for attachment in attachments {
            let reads_only = pass.depth.as_ref().is_some_and(|depth| {
                std::ptr::eq(depth, attachment)
                    && depth.read_only
                    && depth.load == AttachmentLoad::Load
            });
            if self.writes(attachment.resource) && !reads_only {
                return Err(FrameError::OffscreenWrite {
                    pass: pass.entry(),
                    resource: attachment.resource,
                });
            }
        }
        if let Some(write) = pass
            .storage_writes()
            .iter()
            .find(|write| self.writes(write.resource))
        {
            return Err(FrameError::OffscreenWrite {
                pass: pass.entry(),
                resource: write.resource,
            });
        }
        Ok(())
    }
}

/// The groups of one `encode_geometry` call in the order they run, each at the index of the pass
/// it runs before (`passes.len()` for a group that runs after every pass). Groups at one position
/// keep their order in the plan.
///
/// A group runs immediately before the first pass at or after its position that the call
/// encodes, so a group positioned before an entry a depth-fitted prefix already encoded runs at
/// the head of the call that follows the prefix.
pub(crate) struct OperationsSchedule<'a> {
    pending: std::collections::VecDeque<(usize, &'a dyn FrameOperations)>,
}
impl<'a> OperationsSchedule<'a> {
    pub(crate) fn new(passes: &[PassDeclaration], operations: &[OperationsGroup<'a>]) -> Self {
        let mut scheduled: Vec<_> = operations
            .iter()
            .map(|group| (position(passes, group), group.work))
            .collect();
        scheduled.sort_by_key(|(index, _)| *index);
        Self {
            pending: scheduled.into(),
        }
    }

    /// Encodes every group positioned at or before pass `index`, which the caller is about to
    /// encode.
    pub(crate) fn encode_before(
        &mut self,
        index: usize,
        timing: &crate::TimingState,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        while let Some((_, work)) = self.pending.pop_front_if(|(at, _)| *at <= index) {
            work.encode_operations(Some(timing), encoder)?;
        }
        Ok(())
    }

    /// Encodes the groups still waiting, which follow the last pass the call encoded.
    pub(crate) fn encode_rest(
        &mut self,
        timing: &crate::TimingState,
        encoder: &mut wgpu::CommandEncoder,
    ) -> Result<(), FrameError> {
        self.encode_before(usize::MAX, timing, encoder)
    }
}
