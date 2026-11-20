//! GPU work a game asks of its resources: block transfers into declared textures and readbacks
//! of declared textures and targets.
//!
//! Game code runs in `prepare`, `commit` and `finish_draws` and gets no encoder, so the work is
//! queued here in the order asked and the renderer records it into the frame's encoder after
//! the frame's passes ([`GameResources::record_work`]). A transfer asked in a frame is sampled
//! from the next frame on; a readback holds what the frame left. Work recorded into a frame that
//! is cancelled is queued again for the next frame.
//!
//! Readbacks are diagnostics. Each copies one level into a staging buffer of its own, admitted
//! against the factory's budget with everything else the store holds, through the program
//! runtime's readback; nothing waits for it. [`GameResources::take_read_back`] delivers the
//! bytes once the frame has been submitted and completed and the host has polled the device.
use super::{GameResourceError, GameResources, GameTexture};
use crate::compositor::game::GameTargets;
use std::collections::BTreeMap;
use worldrunner_render_gpu::submission::{Publications, TransactionStatus};
use worldrunner_render_post::FrameError;
use worldrunner_render_program::error::ProgramError;
use worldrunner_render_program::runtime::{
    BlockSource, LentBlockDestination, MappingTargetRead, PendingTargetRead, TargetImage,
    TextureLevel,
};

/// One mip level of one layer (or, for a 3D texture, one slice of the level).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct GameLevel {
    pub layer: u32,
    pub mip: u32,
}

impl From<GameLevel> for TextureLevel {
    fn from(level: GameLevel) -> Self {
        Self {
            layer: level.layer,
            mip: level.mip,
        }
    }
}

/// Where a block transfer writes: a level and layer of a declared block-compressed texture,
/// from its first block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameBlockDestination {
    pub texture: GameTexture,
    pub mip: u32,
    pub layer: u32,
    /// The logical texels the blocks describe, which may end inside the last block.
    pub extent: [u32; 2],
}

handle!(
    /// A readback asked of one [`GameResources`], to take once the frame that copied it is done.
    GameReadback,
    "game readback"
);

/// Work asked and not recorded yet, or recorded into a frame not yet submitted.
enum Work {
    Transfer {
        label: String,
        source: BlockSource,
        destination: LentBlockDestination,
    },
    /// The readback of this number in [`WorkQueue::reads`].
    Read(u32),
}

/// Where a readback is.
enum ReadState {
    Queued {
        texture: wgpu::Texture,
        level: TextureLevel,
    },
    /// Copied in `frame`; queued again when the frame is cancelled.
    Recorded {
        texture: wgpu::Texture,
        level: TextureLevel,
        pending: PendingTargetRead,
        frame: Publications,
    },
    Mapping(MappingTargetRead),
    Failed(String),
    /// Moved out while its next state is worked out.
    Taken,
}

struct Read {
    label: String,
    state: ReadState,
}

/// A store's asked work.
#[derive(Default)]
pub(super) struct WorkQueue {
    queued: Vec<Work>,
    /// Work recorded into frames that were not submitted or cancelled yet, with their frame.
    recorded: Vec<(Publications, Work)>,
    /// The readbacks not taken yet, by number; one leaves when it is delivered or fails.
    reads: BTreeMap<u32, Read>,
    /// The number of the next readback.
    next_read: u32,
}

impl WorkQueue {
    /// Moves the work of cancelled frames back to the front of the queue, in the order it was
    /// recorded, and forgets the work of submitted ones.
    fn requeue_cancelled(&mut self) {
        let mut again = Vec::new();
        for (frame, work) in std::mem::take(&mut self.recorded) {
            match frame.status() {
                TransactionStatus::Preparing => self.recorded.push((frame, work)),
                TransactionStatus::Submitted => {}
                TransactionStatus::Cancelled => {
                    if let Work::Read(index) = work
                        && let Some(read) = self.reads.get_mut(&index)
                    {
                        read.state = match std::mem::replace(&mut read.state, ReadState::Taken) {
                            ReadState::Recorded { texture, level, .. } => {
                                ReadState::Queued { texture, level }
                            }
                            other => other,
                        };
                    }
                    again.push(work);
                }
            }
        }
        again.append(&mut self.queued);
        self.queued = again;
    }
}

/// The reason a runtime refusal gives, without the runtime's prefix for readbacks.
fn reason(error: ProgramError) -> String {
    match error {
        ProgramError::Readback(reason) => reason,
        other => other.to_string(),
    }
}

impl GameResources {
    /// Asks for the blocks `source` holds, one texel of an `Rg32Uint` or `Rgba32Uint` image of
    /// the runtime per block, to be moved into a level and layer of a declared block-compressed
    /// texture. The renderer records the transfer after the passes of the frame being prepared,
    /// or of the next frame when none is.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's texture and
    /// [`GameResourceError::Transfer`] for a texture that is not block-compressed or a mip or
    /// layer outside it. The rest of the transfer's rules (the source's format, usage and
    /// region, the extent) are checked as the frame records it, which fails then.
    pub fn transfer_block_image(
        &self,
        source: &BlockSource,
        destination: GameBlockDestination,
    ) -> Result<(), GameResourceError> {
        let mut store = self.store.borrow_mut();
        let declared = self.texture(&store, destination.texture)?;
        let texture = (*declared.texture).clone();
        let label = declared.label.clone();
        let refuse = |reason: String| {
            Err(GameResourceError::Transfer {
                label: label.clone(),
                reason,
            })
        };
        if !texture.format().is_compressed() {
            return refuse(format!("{:?} is not block-compressed", texture.format()));
        }
        if destination.mip >= texture.mip_level_count() {
            return refuse(format!(
                "mip {} is outside the {} mips of the texture",
                destination.mip,
                texture.mip_level_count()
            ));
        }
        if destination.layer >= texture.depth_or_array_layers() {
            return refuse(format!(
                "layer {} is outside the {} layers of the texture",
                destination.layer,
                texture.depth_or_array_layers()
            ));
        }
        store.work.queued.push(Work::Transfer {
            label,
            source: *source,
            destination: LentBlockDestination {
                texture,
                mip: destination.mip,
                layer: destination.layer,
                extent: destination.extent,
            },
        });
        Ok(())
    }

    /// Asks for a copy of one level of a declared texture, taken with
    /// [`Self::take_read_back`] after the frame. The renderer records it after the passes of
    /// the frame being prepared, or of the next frame when none is, so it holds what that frame
    /// left, including transfers asked before it.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's texture,
    /// [`GameResourceError::Readback`] for a level outside the texture or a format that cannot
    /// be copied, and [`GameResourceError::Budget`] when the staging does not fit the budget.
    pub fn read_back_texture(
        &self,
        texture: GameTexture,
        level: GameLevel,
    ) -> Result<GameReadback, GameResourceError> {
        let (label, texture) = {
            let store = self.store.borrow();
            let declared = self.texture(&store, texture)?;
            (declared.label.clone(), (*declared.texture).clone())
        };
        self.read_back(label, texture, level.into())
    }

    /// Asks for a copy of one level of the target `name` of `targets`, as
    /// [`Self::read_back_texture`] does for a declared texture. The target must be declared with
    /// `COPY_SRC`.
    ///
    /// # Errors
    /// [`GameResourceError::Readback`] for an undeclared target, a level outside it or a target
    /// without `COPY_SRC`, and [`GameResourceError::Budget`] when the staging does not fit.
    pub fn read_back_target(
        &self,
        targets: &GameTargets,
        name: &'static str,
        level: GameLevel,
    ) -> Result<GameReadback, GameResourceError> {
        let texture = targets
            .texture(name)
            .map_err(|error| GameResourceError::Readback {
                label: name.to_owned(),
                reason: error.to_string(),
            })?;
        self.read_back(name.to_owned(), texture, level.into())
    }

    fn read_back(
        &self,
        label: String,
        texture: wgpu::Texture,
        level: TextureLevel,
    ) -> Result<GameReadback, GameResourceError> {
        let bytes =
            level
                .readback_bytes(&texture)
                .map_err(|error| GameResourceError::Readback {
                    label: label.clone(),
                    reason: reason(error),
                })?;
        self.fits(&label, bytes)?;
        let mut store = self.store.borrow_mut();
        let work = &mut store.work;
        let index = work.next_read;
        work.next_read = index.checked_add(1).expect("fewer than 2^32 readbacks");
        work.reads.insert(
            index,
            Read {
                label,
                state: ReadState::Queued { texture, level },
            },
        );
        work.queued.push(Work::Read(index));
        Ok(GameReadback {
            store: self.id,
            index,
        })
    }

    /// The level a readback copied, once the frame that recorded the copy has been submitted
    /// and completed on the GPU and the host has polled the device; `None` until then. It is
    /// delivered once, and its staging is given back then.
    ///
    /// # Errors
    /// [`GameResourceError::Foreign`] for another store's readback, and
    /// [`GameResourceError::Readback`] when the copy could not be made or mapped, or was already
    /// delivered.
    pub fn take_read_back(
        &self,
        readback: GameReadback,
    ) -> Result<Option<TargetImage>, GameResourceError> {
        self.slot(readback.store, readback.index, GameReadback::KIND)?;
        let mut store = self.store.borrow_mut();
        let reads = &mut store.work.reads;
        let Some(read) = reads.get_mut(&readback.index) else {
            return Err(GameResourceError::Readback {
                label: format!("{} {}", GameReadback::KIND, readback.index),
                reason: "it was already delivered or failed".to_owned(),
            });
        };
        let refuse = |label: &str, reason: String| GameResourceError::Readback {
            label: label.to_owned(),
            reason,
        };
        let submitted = matches!(
            &read.state,
            ReadState::Recorded { frame, .. } if frame.status() == TransactionStatus::Submitted
        );
        if submitted
            && let ReadState::Recorded { pending, .. } =
                std::mem::replace(&mut read.state, ReadState::Taken)
        {
            read.state = pending
                .begin()
                .map_or_else(|error| ReadState::Failed(reason(error)), ReadState::Mapping);
        }
        let outcome = match std::mem::replace(&mut read.state, ReadState::Taken) {
            ReadState::Mapping(mut mapping) => match mapping.try_finish() {
                Ok(Some(image)) => Ok(Some(image)),
                Ok(None) => {
                    read.state = ReadState::Mapping(mapping);
                    Ok(None)
                }
                Err(error) => Err(refuse(&read.label, reason(error))),
            },
            ReadState::Failed(why) => Err(refuse(&read.label, why)),
            waiting => {
                read.state = waiting;
                Ok(None)
            }
        };
        if !matches!(outcome, Ok(None)) {
            reads.remove(&readback.index);
        }
        outcome
    }

    /// Records the work asked since the last frame into `encoder`'s frame, after its passes:
    /// what was recorded into cancelled frames first, then the rest in the order asked. A
    /// readback that cannot be recorded fails its ticket; a transfer that cannot fails the
    /// frame, and the work asked after it waits for the next frame.
    ///
    /// # Errors
    /// [`FrameError`] when the encoder is not a frame being recorded, and when a transfer is
    /// refused.
    pub(crate) fn record_work(&self, encoder: &mut wgpu::CommandEncoder) -> Result<(), FrameError> {
        let frame = Publications::for_encoder(encoder)?;
        let queued = {
            let mut store = self.store.borrow_mut();
            store.work.requeue_cancelled();
            std::mem::take(&mut store.work.queued)
        };
        let mut queued = queued.into_iter();
        for work in queued.by_ref() {
            let recorded = match work {
                Work::Transfer {
                    label,
                    source,
                    destination,
                } => self
                    .runtime
                    .borrow()
                    .transfer_block_image_into(encoder, &source, &destination)
                    .map(|()| {
                        Some(Work::Transfer {
                            label: label.clone(),
                            source,
                            destination,
                        })
                    })
                    .map_err(|error| GameResourceError::Transfer {
                        label,
                        reason: error.to_string(),
                    }),
                Work::Read(index) => Ok(self.record_read(encoder, &frame, index)),
            };
            match recorded {
                Ok(Some(work)) => self
                    .store
                    .borrow_mut()
                    .work
                    .recorded
                    .push((frame.clone(), work)),
                Ok(None) => {}
                Err(error) => {
                    self.store.borrow_mut().work.queued.extend(queued);
                    return Err(error.into());
                }
            }
        }
        Ok(())
    }

    /// Records the copy of readback `index` and returns its work, or fails its ticket.
    fn record_read(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        frame: &Publications,
        index: u32,
    ) -> Option<Work> {
        let (label, queued) = {
            let mut store = self.store.borrow_mut();
            let read = store
                .work
                .reads
                .get_mut(&index)
                .expect("a queued readback is held until it is taken");
            let state = std::mem::replace(&mut read.state, ReadState::Taken);
            (read.label.clone(), state)
        };
        let ReadState::Queued { texture, level } = queued else {
            unreachable!("only a queued readback is in the queue")
        };
        let state = level
            .readback_bytes(&texture)
            .map_err(reason)
            .and_then(|bytes| self.fits(&label, bytes).map_err(|error| error.to_string()))
            .and_then(|()| {
                self.runtime
                    .borrow()
                    .read_lent_texture(encoder, &texture, level)
                    .map_err(reason)
            })
            .map_or_else(ReadState::Failed, |pending| ReadState::Recorded {
                texture,
                level,
                pending,
                frame: frame.clone(),
            });
        let recorded = matches!(state, ReadState::Recorded { .. });
        self.store
            .borrow_mut()
            .work
            .reads
            .get_mut(&index)
            .expect("a queued readback is held until it is taken")
            .state = state;
        recorded.then_some(Work::Read(index))
    }
}

#[cfg(test)]
#[path = "resource_work_tests.rs"]
mod tests;
