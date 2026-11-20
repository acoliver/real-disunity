//! Commands a feature prepared in full, such as a program runtime's dispatch or block transfer,
//! staged on the frame's preparation and recorded in staging order with the context's own copies
//! and dispatches, and the views of the frame they write.
//!
//! A view a preparation writes is declared by the plan (`AttachmentView::with_prepared_contents`),
//! so the frame's validation counts it as produced before the first pass, the stock compile
//! versions its write like an attachment's output, and a capture that submits the frame checks it
//! owns the written allocation. Staging then has to agree with the plan: every write it stages was
//! planned, every planned write is staged exactly once, and the views staged commands declare
//! they write are the views they report writing.
use std::ops::Range;
use std::sync::Arc;

use super::{AttachmentView, FrameError, PrepareContext, ResourceRef, Staged};

/// Commands a feature prepared in full: everything that can fail was checked when the value was
/// made, so recording them can only record.
///
/// Stage them with [`PrepareContext::record_prepared`]. A pipeline and bind group the feature
/// built itself, over resources that are not views of the frame and that it keeps alive itself,
/// can instead be staged with [`PrepareContext::dispatch`], which checks and retains nothing.
pub trait PreparedCommands {
    /// The subresources the commands write, among every texture they write. The frame compares
    /// the ones that are its views with the access the commands were staged with.
    fn writes(&self) -> Vec<PreparedWrite>;
    /// Makes the frame `encoder` records hold what the commands use until the frame is cancelled,
    /// dropped or completed. The frame calls it once, when the commands are staged.
    ///
    /// # Errors
    /// The owner's refusal of the retention; nothing is staged then.
    fn retain(&self, encoder: &wgpu::CommandEncoder) -> Result<(), PreparedRefusal>;
    /// Records the commands. Called when the frame's preparation finishes, in staging order, with
    /// the `label` they were staged as.
    ///
    /// The commands name themselves with `label` in GPU captures: a compute or render pass they
    /// open takes it as the pass label, and commands outside a pass go inside an encoder debug
    /// group of that name. A debug group pushed on the encoder around a pass costs a command buffer
    /// of its own when it is popped: wgpu ends the encoder's command buffer at each pass and opens
    /// another for the next command outside one.
    ///
    /// # Errors
    /// The owner's refusal to record them, such as a GPU validation error it guards against;
    /// the frame then fails with it.
    fn record(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
    ) -> Result<(), PreparedRefusal>;
}

/// Mips and array layers of one texture that prepared commands write.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedWrite {
    pub texture: wgpu::Texture,
    pub mips: Range<u32>,
    pub layers: Range<u32>,
}

impl PreparedWrite {
    /// One mip of one layer of `texture`.
    #[must_use]
    pub fn layer(texture: &wgpu::Texture, mip: u32, layer: u32) -> Self {
        Self {
            texture: texture.clone(),
            mips: mip..mip + 1,
            layers: layer..layer + 1,
        }
    }
}

/// The refusal of the owner of prepared commands to retain or record them, kept as the owner's
/// own error: [`downcast_ref`](Self::downcast_ref) recovers it, such as a program runtime's
/// `ProgramError`. Two refusals are equal when they are the same refusal.
#[derive(Clone, Debug)]
pub struct PreparedRefusal(Arc<dyn std::error::Error + Send + Sync>);

impl PreparedRefusal {
    #[must_use]
    pub fn new(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self(Arc::new(error))
    }

    /// The owner's error, when it is an `E`.
    pub fn downcast_ref<E: std::error::Error + 'static>(&self) -> Option<&E> {
        self.0.downcast_ref()
    }
}

impl std::fmt::Display for PreparedRefusal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

impl std::error::Error for PreparedRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl PartialEq for PreparedRefusal {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for PreparedRefusal {}

/// What one staged operation reads and writes among the frame's views. Resources the operation
/// owns itself, such as a runtime's scratch images and staging buffers, are not views of the frame
/// and are not named.
#[derive(Clone, Copy, Debug, Default)]
pub struct PreparedAccess<'a> {
    /// Views whose contents an earlier submitted frame left (retained views).
    pub reads: &'a [ResourceRef],
    /// Views the plan declared written in preparation: exactly the views of the frame the
    /// commands report writing ([`PreparedCommands::writes`]).
    pub writes: &'a [ResourceRef],
}

impl AttachmentView {
    /// Declares that a staged preparation operation of this frame writes this view's subresource,
    /// by copy, before the first pass the frame encodes. Passes may read it as if a producer wrote
    /// it. It declares no contents from an earlier submission, so the texture is imported at the
    /// same initial version as without it.
    #[must_use]
    pub fn with_prepared_contents(mut self) -> Self {
        self.prepared = true;
        self
    }

    /// Whether a staged preparation operation of this frame writes this view.
    pub fn prepared_contents(&self) -> bool {
        self.prepared
    }
}

impl PrepareContext<'_> {
    /// Stages `commands`, recorded after the copies, dispatches and commands staged before it and
    /// labelled `label` ([`PreparedCommands::record`]). Calls `commands.retain` first.
    ///
    /// Use this for work that writes or reads views of the frame, and for work whose resources
    /// the frame must hold until it completes, such as a program runtime's prepared dispatch or
    /// block transfer: the frame checks `access` against the plan and against what the commands
    /// report writing, and versions their writes. [`Self::dispatch`] stages a pipeline and bind
    /// group as they are, with no access, checks or retention, so it is only for a feature's own
    /// resources that are not views of the frame.
    ///
    /// # Errors
    /// [`FrameError::PreparedOutsidePreparation`] outside the features' `prepare`;
    /// [`FrameError::AttachmentView`] for a read or write the frame's views lack;
    /// [`FrameError::UnplannedWrite`] for a write whose view the plan did not declare prepared;
    /// [`FrameError::RepeatedPreparedWrite`] for a view the frame's preparation already wrote or
    /// that `writes` names twice; [`FrameError::MissingProducer`] for a read whose view holds no
    /// retained contents; [`FrameError::AttachmentHazard`] for a read of a view the preparation
    /// writes and for a view a pass of the frame's completed geometry prefix attaches;
    /// [`FrameError::PreparedWriteMismatch`] when the views of the frame `commands` write are not
    /// the views `writes` names; [`FrameError::PreparedRefused`] with the owner's refusal of the
    /// retention. Nothing is staged on any refusal.
    pub fn record_prepared(
        &mut self,
        label: &'static str,
        access: PreparedAccess<'_>,
        commands: Box<dyn PreparedCommands>,
    ) -> Result<(), FrameError> {
        if !self.preparing {
            return Err(FrameError::PreparedOutsidePreparation);
        }
        self.check_access(access)?;
        self.check_writes(label, access.writes, &commands.writes())?;
        commands
            .retain(self.encoder)
            .map_err(|refusal| FrameError::PreparedRefused { label, refusal })?;
        self.prepared_written.extend_from_slice(access.writes);
        self.staged.push(Staged::Prepared { label, commands });
        Ok(())
    }

    /// The views the plan declared written in preparation, for bookkeeping that must count them.
    pub fn prepared_views(&self) -> impl Iterator<Item = &AttachmentView> + '_ {
        self.views.iter().filter(|view| view.prepared)
    }

    fn check_access(&self, access: PreparedAccess<'_>) -> Result<(), FrameError> {
        for (index, &resource) in access.writes.iter().enumerate() {
            if !self.resource(resource)?.prepared {
                return Err(FrameError::UnplannedWrite(resource));
            }
            if self.prepared_written.contains(&resource)
                || access.writes[..index].contains(&resource)
            {
                return Err(FrameError::RepeatedPreparedWrite(resource));
            }
            self.refuse_prefix(resource)?;
        }
        for &resource in access.reads {
            let view = self.resource(resource)?;
            if view.prepared {
                return Err(FrameError::AttachmentHazard(resource));
            }
            if !view.retained {
                return Err(FrameError::MissingProducer(resource));
            }
            self.refuse_prefix(resource)?;
        }
        Ok(())
    }

    /// Refuses commands whose writes of the frame's views are not `declared`. A write of a
    /// texture none of the frame's views show is the commands' owner's own and is not compared; a
    /// write of a subresource of a frame texture that no view shows is named as the frame names
    /// that texture's views, with the written mip and layer.
    fn check_writes(
        &self,
        label: &'static str,
        declared: &[ResourceRef],
        writes: &[PreparedWrite],
    ) -> Result<(), FrameError> {
        let mut written = Vec::new();
        for write in writes {
            let on_texture: Vec<ResourceRef> = self
                .views
                .iter()
                .filter(|view| *view.texture() == write.texture)
                .map(AttachmentView::resource)
                .collect();
            let Some(&named) = on_texture.first() else {
                continue;
            };
            for mip in write.mips.clone() {
                for layer in write.layers.clone() {
                    let shown = on_texture
                        .iter()
                        .find(|view| view.mip == mip && view.layer == layer);
                    written.push(shown.copied().unwrap_or(ResourceRef {
                        mip,
                        layer,
                        ..named
                    }));
                }
            }
        }
        written.sort_unstable();
        written.dedup();
        let mut expected = declared.to_vec();
        expected.sort_unstable();
        if written != expected {
            return Err(FrameError::PreparedWriteMismatch {
                label,
                declared: declared.to_vec(),
                written,
            });
        }
        Ok(())
    }

    /// A pass the frame's completed geometry prefix encoded ran before anything staged here, so a
    /// prepared command may not use what such a pass attaches.
    fn refuse_prefix(&self, resource: ResourceRef) -> Result<(), FrameError> {
        let attached = self
            .passes
            .iter()
            .filter(|pass| self.completed.contains(&pass.entry()))
            .flat_map(|pass| {
                pass.color
                    .iter()
                    .chain(&pass.additional_colors)
                    .chain(&pass.depth)
            })
            .any(|attachment| attachment.resource == resource);
        if attached {
            return Err(FrameError::AttachmentHazard(resource));
        }
        Ok(())
    }

    /// Opens the features' preparation, the only time prepared commands may be staged.
    pub(crate) fn open_preparation(&mut self) {
        self.preparing = true;
    }

    /// Opens the features' preparation of the validated `plan`, after the passes of its geometry
    /// prefix that `completed` names.
    pub(crate) fn open_preparation_of(
        &mut self,
        plan: &crate::FramePlan<'_>,
        completed: &[crate::PassEntry],
    ) {
        self.set_views(plan.views);
        self.set_passes(plan.passes);
        self.set_completed_prefix(completed);
        self.open_preparation();
    }

    /// Closes the features' preparation: every write the plan declared prepared was staged, and
    /// the capture the frame belongs to, if any, owns each allocation they write.
    ///
    /// # Errors
    /// [`FrameError::UnstagedWrite`] for a planned write no preparation staged, and the capture's
    /// refusal of a write it does not own.
    pub(crate) fn close_preparation(
        &mut self,
        raster: &crate::contracts::StockRaster,
    ) -> Result<(), FrameError> {
        self.preparing = false;
        if let Some(view) = self
            .prepared_views()
            .find(|view| !self.prepared_written.contains(&view.resource()))
        {
            return Err(FrameError::UnstagedWrite(view.resource()));
        }
        raster.validate_prepared_writes(self.encoder)?;
        Ok(())
    }
}

/// Records staged prepared commands, which label themselves `label`.
pub(super) fn record(
    encoder: &mut wgpu::CommandEncoder,
    label: &'static str,
    commands: &dyn PreparedCommands,
) -> Result<(), FrameError> {
    commands
        .record(encoder, label)
        .map_err(|refusal| FrameError::PreparedRefused { label, refusal })
}
