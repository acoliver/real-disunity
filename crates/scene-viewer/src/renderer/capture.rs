use super::{Renderer, live::Capture};
mod accumulation;
pub(super) mod advanced;
mod products;
use crate::{RetainedHost, ViewerError};
pub(super) use products::Products;
use worldrunner_render_gpu::flight::CompletionTicket;

struct Pending {
    id: u64,
    frame: u64,
    time: std::time::Duration,
    ticket: CompletionTicket,
    image: Products,
}

#[derive(Default)]
pub(super) struct Captures {
    pending: Vec<Pending>,
    spare: Vec<Capture>,
    frame: u64,
    pub(super) active: Option<advanced::Active>,
}
impl Captures {
    pub(super) fn next_frame(&self) -> u64 {
        self.frame
    }
    pub(super) fn advance_frame(&mut self) {
        self.frame = self
            .frame
            .checked_add(1)
            .expect("render frame identity overflow");
    }
    pub(super) fn is_pending(&self) -> bool {
        !self.pending.is_empty() || self.active.is_some()
    }

    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        host: &mut RetainedHost,
        texture: &wgpu::Texture,
        scheduler: &worldrunner_render_scene::compositor::ConfiguredFrameRenderer,
        pages_ready: bool,
    ) -> Result<Option<(u64, Products)>, ViewerError> {
        if self.pending.len() == 3 {
            return Ok(None);
        }
        let Some(&(id, _, _)) = host.captures.waiting.iter().find(|r| {
            host.capture_ready(r, pages_ready)
                && !host
                    .captures
                    .recipes
                    .get(&r.0)
                    .is_some_and(crate::RetainedCaptureRecipe::accumulated)
        }) else {
            return Ok(None);
        };
        let scene = scheduler.targets().texture();
        host.captures
            .resize_reservation(
                id,
                [
                    texture.width().max(scene.width()),
                    texture.height().max(scene.height()),
                ],
            )
            .map_err(|error| ViewerError::Driver(error.to_string()))?;
        let outputs = host
            .captures
            .recipes
            .get(&id)
            .map_or(&[crate::CaptureOutput::Srgb8][..], |recipe| {
                recipe.outputs()
            });
        let mut image = Products::new(device, texture, scene, outputs, self.spare.pop())?;
        image.features(device, scheduler, outputs)?;
        Ok(Some((id, image)))
    }

    pub(super) fn submitted(
        &mut self,
        host: &mut RetainedHost,
        image: Option<(u64, Products)>,
        ticket: CompletionTicket,
        time: std::time::Duration,
    ) -> Result<(), ViewerError> {
        let frame = self.frame;
        self.advance_frame();
        if let Some((id, mut image)) = image {
            let index = host
                .captures
                .waiting
                .iter()
                .position(|r| r.0 == id)
                .expect("capture request");
            host.captures.waiting.remove(index);
            image.begin()?;
            self.pending.push(Pending {
                id,
                frame,
                time,
                ticket,
                image,
            });
        }
        Ok(())
    }

    pub(super) fn collect(&mut self, host: &mut RetainedHost) -> Result<(), ViewerError> {
        if host.captures.waiting.is_empty() && self.active.is_some() {
            self.active = None;
            return Ok(());
        }
        if let Some(active) = &mut self.active
            && active.collect(host)?
        {
            self.active
                .take()
                .expect("completed accumulation")
                .finish(host);
        }
        let mut index = 0;
        while index < self.pending.len() {
            let pending = &mut self.pending[index];
            if pending.ticket.is_complete()
                && let Some(image) = pending.image.try_read()?
            {
                if !host.captures.accepts_result(pending.id) {
                    // Cancellation delivery must not publish a second result.
                } else if host.captures.recipes.contains_key(&pending.id) {
                    host.captures
                        .complete_recipe(pending.id, pending.frame, pending.time, image);
                } else {
                    host.captures.complete(
                        pending.id,
                        pending.frame,
                        pending.time,
                        image.srgb8.expect("plain capture"),
                    );
                }
                let pending = self.pending.remove(index);
                // The map and GPU ticket have both finished before slot reuse.
                if self.spare.len() < 3
                    && let Some(display) = pending.image.display
                {
                    self.spare.push(display);
                }
            } else {
                index += 1;
            }
        }
        Ok(())
    }
}

impl Renderer {
    pub(crate) fn drain(&self) -> Result<(), ViewerError> {
        self.context
            .device()
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(30)),
            })
            .map_err(|error| ViewerError::Gpu(error.to_string()))?;
        self.check_errors()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod feature_tests;
